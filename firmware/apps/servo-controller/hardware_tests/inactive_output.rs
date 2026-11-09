#![no_std]
#![no_main]

//! Manual test: inactive PIO/DMA initialization without bootstrap pulses.
//!
//! Observe unloaded GP0 and GP1 against Pico GND.
//! UART1: GP20 -> TTL RXD, common GND, 3.3 V, 115200 baud, 8N1.
//!
//! Synthetic calibration: -45/0/+45 degrees -> 1000/1500/2000 us.
//!
//! Expected sequence:
//! - From initialization: both outputs Low.
//! - Two seconds prepared: both outputs Low, LED on.
//! - Five seconds running: GP0 1 ms, GP1 2 ms, LED blinking.
//! - Two seconds stopped: both outputs Low, LED on.
//! - Then running: GP0 1.5 ms, GP1 2 ms, LED blinking.
//!
//! No pulse may occur before the first explicit bank.start_prepared().
//! No servos or mechanical loads are connected.

mod support;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_control::{
    Angle, Calibration, CalibrationError, ServoBank, ServoBankError, ServoId,
    ServoSignalOutput, ServoTarget,
};
use hexaroam_servo_pio::{OutputBuffer, PreparedOutput, SwitchingOutput};
use hexaroam_servo_signal::{PulseLimits, SignalOutputState};
use rp_pico::{
    entry,
    hal::{
        clocks::Clock,
        dma::DMAExt,
        gpio::{FunctionPio0, FunctionUart, PullNone},
        pio::PIOExt,
        uart::{Pins, UartConfig, UartPeripheral},
    },
};
use uart_logging::UartLogOutput;

#[derive(Clone, Copy)]
enum Phase {
    Prepared,
    Running,
    Stopped,
    Restarted,
}

fn angle(centidegrees: i32) -> Angle {
    Angle::from_centidegrees(centidegrees)
}

fn target(id: usize, centidegrees: i32) -> ServoTarget {
    ServoTarget {
        id: ServoId(id),
        angle: angle(centidegrees),
    }
}

#[entry]
fn main() -> ! {
    let mut board = support::initialize();

    let uart_pins = Pins::default().tx(board
        .pins
        .gpio20
        .into_function::<FunctionUart>());

    let uart = UartPeripheral::new(board.uart, uart_pins, &mut board.resets)
        .enable(
            UartConfig::default(),
            board.peripheral_clock.freq(),
        )
        .expect("Failed to initialize UART");

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(uart));

    let calibration = Calibration::new(
        angle(-4_500),
        1_000,
        angle(4_500),
        2_000,
    )
    .expect("Valid synthetic calibration");

    let limits = PulseLimits::new(1_000, 2_000).expect("Valid pulse limits");

    let (mut pio, sm0, _, _, _) = board.pio.split(&mut board.resets);
    let dma = board.dma.split(&mut board.resets);

    let pins = [
        board
            .pins
            .gpio0
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
        board
            .pins
            .gpio1
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
    ];

    let first_buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("First buffer already allocated");

    let second_buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("Second buffer already allocated");

    pio.clear_irq(1);

    let prepared = PreparedOutput::new(
        &mut pio,
        sm0,
        pins,
        board.system_clock_hz,
    )
    .expect("Failed to prepare outputs");

    let mut output = SwitchingOutput::new(
        prepared,
        dma.ch0,
        first_buffer,
        second_buffer,
        20_000,
    )
    .expect("Failed to initialize inactive output");

    // No transfer or PIO execution has been started.
    assert_eq!(
        output.state(),
        SignalOutputState::Inactive
    );
    assert!(!output.dma_is_active());
    assert!(!output.is_faulted());

    // Servicing an inactive output must not implicitly start it.
    output.service();

    assert_eq!(
        output.state(),
        SignalOutputState::Inactive
    );
    assert!(!output.dma_is_active());
    assert!(!output.is_faulted());

    let adapter =
        ServoSignalOutput::<_, 2, 3>::new(output, [0, 1], limits, 20_000)
            .expect("Valid adapter configuration");

    let mut bank = ServoBank::new([calibration, calibration], adapter);

    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );

    // Missing targets must prevent startup.
    assert_eq!(
        bank.start_prepared(),
        Err(ServoBankError::UninitializedServo(
            ServoId(0)
        )),
    );
    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );

    bank.prepare_targets(&[target(0, -4_500)])
        .expect("First target must be valid");

    assert_eq!(
        bank.start_prepared(),
        Err(ServoBankError::UninitializedServo(
            ServoId(1)
        )),
    );
    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );

    bank.prepare_targets(&[target(1, 4_500)])
        .expect("Second target must be valid");

    // An invalid later entry must also reject the preceding valid change.
    assert_eq!(
        bank.prepare_targets(&[target(0, 0), target(1, 4_501),]),
        Err(ServoBankError::Calibration {
            id: ServoId(1),
            error: CalibrationError::AngleOutOfRange,
        }),
    );

    assert_eq!(
        bank.commanded_angle(ServoId(0)),
        Ok(Some(angle(-4_500))),
    );
    assert_eq!(
        bank.commanded_angle(ServoId(1)),
        Ok(Some(angle(4_500))),
    );
    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );

    logger
        .log(
            LogLevel::Info,
            "Bank lifecycle: prepared; output inactive",
        )
        .expect("Test message must fit");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_high()
        .expect("Failed to turn LED on");

    let mut phase = Phase::Prepared;
    let mut phase_started_us = board.timer.get_counter_low();
    let mut last_toggle_us = phase_started_us;
    let mut led_high = true;

    loop {
        bank.service();

        let now_us = board.timer.get_counter_low();

        match phase {
            Phase::Prepared => {
                assert_eq!(
                    bank.state(),
                    SignalOutputState::Inactive
                );

                if now_us.wrapping_sub(phase_started_us) >= 2_000_000 {
                    bank.start_prepared()
                        .expect("Prepared startup must succeed");

                    assert_eq!(bank.state(), SignalOutputState::Active);

                    logger
                        .log(
                            LogLevel::Info,
                            "Bank lifecycle: initial angles running",
                        )
                        .expect("Test message must fit");

                    led.set_low()
                        .expect("Failed to turn LED off");
                    led_high = false;

                    phase = Phase::Running;
                    phase_started_us = board.timer.get_counter_low();
                    last_toggle_us = phase_started_us;
                }
            }
            Phase::Running => {
                assert_eq!(bank.state(), SignalOutputState::Active);

                if now_us.wrapping_sub(phase_started_us) >= 5_000_000 {
                    bank.stop()
                        .expect("Bank stop must succeed");
                    bank.stop()
                        .expect("Repeated stop must succeed");

                    assert_eq!(
                        bank.state(),
                        SignalOutputState::Inactive
                    );
                    assert_eq!(
                        bank.commanded_angle(ServoId(0)),
                        Ok(Some(angle(-4_500))),
                    );
                    assert_eq!(
                        bank.commanded_angle(ServoId(1)),
                        Ok(Some(angle(4_500))),
                    );

                    // Prepare just one changed target while stopped.
                    bank.prepare_targets(&[target(0, 0)])
                        .expect("Stopped target update must succeed");

                    assert_eq!(
                        bank.commanded_angle(ServoId(0)),
                        Ok(Some(angle(0))),
                    );
                    assert_eq!(
                        bank.commanded_angle(ServoId(1)),
                        Ok(Some(angle(4_500))),
                    );
                    assert_eq!(
                        bank.state(),
                        SignalOutputState::Inactive
                    );

                    logger
                        .log(
                            LogLevel::Info,
                            "Bank lifecycle: stopped; servo 0 changed",
                        )
                        .expect("Test message must fit");

                    led.set_high()
                        .expect("Failed to turn LED on");
                    led_high = true;

                    phase = Phase::Stopped;
                    phase_started_us = board.timer.get_counter_low();
                }
            }
            Phase::Stopped => {
                assert_eq!(
                    bank.state(),
                    SignalOutputState::Inactive
                );

                if now_us.wrapping_sub(phase_started_us) >= 2_000_000 {
                    bank.start_prepared()
                        .expect("Bank restart must succeed");

                    assert_eq!(bank.state(), SignalOutputState::Active);
                    assert_eq!(
                        bank.commanded_angle(ServoId(0)),
                        Ok(Some(angle(0))),
                    );
                    assert_eq!(
                        bank.commanded_angle(ServoId(1)),
                        Ok(Some(angle(4_500))),
                    );

                    logger
                        .log(
                            LogLevel::Info,
                            "Bank lifecycle OK: updated angles running",
                        )
                        .expect("Test message must fit");

                    led.set_low()
                        .expect("Failed to turn LED off");
                    led_high = false;

                    phase = Phase::Restarted;
                    last_toggle_us = board.timer.get_counter_low();
                }
            }
            Phase::Restarted => {
                assert_eq!(bank.state(), SignalOutputState::Active);
            }
        }

        // Refresh after phase transitions that sampled the timer again.
        let now_us = board.timer.get_counter_low();

        if matches!(phase, Phase::Running | Phase::Restarted)
            && now_us.wrapping_sub(last_toggle_us) >= 500_000
        {
            last_toggle_us = now_us;
            led_high = !led_high;

            if led_high {
                led.set_high()
                    .expect("Failed to turn LED on");
            } else {
                led.set_low()
                    .expect("Failed to turn LED off");
            }
        }

        logger
            .service(16)
            .expect("UART output is infallible");
    }
}
