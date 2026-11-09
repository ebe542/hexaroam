#![no_std]
#![no_main]

//! Manual hardware test: calibrated angle batches through ServoBank.
//!
//! Observe unloaded GP0 and GP1 against Pico GND.
//! UART1: GP20 -> TTL RXD, common GND, 3.3 V, 115200 baud, 8N1.
//!
//! Synthetic calibration: -45/0/+45 degrees -> 1000/1500/2000 us.
//! Each step lasts approximately two seconds.
//!
//! Expected GP0/GP1 pulse widths:
//! 1500/1500 -> 1000/2000 -> 1500/2000 -> 1500/1000
//! -> unchanged after invalid request -> 1500/1500 -> repeat.
//!
//! Rising edges remain synchronous. Nominal period: 20 ms.
//! No physical servo position or calibration is verified.

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
use hexaroam_servo_signal::{PulseLimits, PulseTarget, SignalPlan};
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

enum Step {
    OppositeAngles,
    FirstServoCenter,
    SecondServoLower,
    InvalidRequest,
    BothCenter,
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

    let center_pulse_us = calibration
        .pulse_width_us(angle(0))
        .expect("Center angle must be valid");

    // SwitchingOutput needs an initial plan before it can be wrapped.
    // Use the same calibration and center targets as the bank startup.
    let initial_plan = SignalPlan::<3>::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: center_pulse_us,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: center_pulse_us,
            },
        ],
        &limits,
        20_000,
    )
    .expect("Valid initial plan");

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

    let output = SwitchingOutput::start(
        prepared,
        dma.ch0,
        first_buffer,
        second_buffer,
        &initial_plan,
    )
    .expect("Failed to start output");

    let adapter =
        ServoSignalOutput::<_, 2, 3>::new(output, [0, 1], limits, 20_000)
            .expect("Valid adapter configuration");

    let mut bank = ServoBank::new([calibration, calibration], adapter);

    bank.set_targets(&[target(0, 0), target(1, 0)])
        .expect("Initial complete request must succeed");

    logger
        .log(
            LogLevel::Info,
            "Accepted: both servos at 0 degrees",
        )
        .expect("Test message must fit");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut led_high = false;
    let mut step = Step::OppositeAngles;
    let mut last_step_us = board.timer.get_counter_low();
    let mut last_toggle_us = last_step_us;

    loop {
        bank.service();

        let now_us = board.timer.get_counter_low();

        if now_us.wrapping_sub(last_step_us) >= 2_000_000 {
            last_step_us = now_us;

            let message = match step {
                Step::OppositeAngles => {
                    bank.set_targets(&[target(0, -4_500), target(1, 4_500)])
                        .expect("Complete angle update must succeed");

                    step = Step::FirstServoCenter;
                    "Accepted: servo 0=-45, servo 1=+45"
                }
                Step::FirstServoCenter => {
                    bank.set_targets(&[target(0, 0)])
                        .expect("Single-servo update must succeed");

                    assert_eq!(
                        bank.commanded_angle(ServoId(1)),
                        Ok(Some(angle(4_500))),
                    );

                    step = Step::SecondServoLower;
                    "Accepted: servo 0=0; servo 1 unchanged"
                }
                Step::SecondServoLower => {
                    bank.set_targets(&[target(1, -4_500)])
                        .expect("Single-servo update must succeed");

                    assert_eq!(
                        bank.commanded_angle(ServoId(0)),
                        Ok(Some(angle(0))),
                    );

                    step = Step::InvalidRequest;
                    "Accepted: servo 1=-45; servo 0 unchanged"
                }
                Step::InvalidRequest => {
                    assert_eq!(
                        bank.set_targets(&[
                            target(0, -4_500),
                            target(1, 4_600),
                        ]),
                        Err(ServoBankError::Calibration {
                            id: ServoId(1),
                            error: CalibrationError::AngleOutOfRange,
                        }),
                    );

                    assert_eq!(
                        bank.commanded_angle(ServoId(0)),
                        Ok(Some(angle(0))),
                    );
                    assert_eq!(
                        bank.commanded_angle(ServoId(1)),
                        Ok(Some(angle(-4_500))),
                    );

                    step = Step::BothCenter;
                    "Rejected invalid batch; both commands unchanged"
                }
                Step::BothCenter => {
                    bank.set_targets(&[target(0, 0), target(1, 0)])
                        .expect("Complete center update must succeed");

                    step = Step::OppositeAngles;
                    "Accepted: both servos at 0 degrees"
                }
            };

            logger
                .log(LogLevel::Info, message)
                .expect("Test message must fit");
        }

        logger
            .service(16)
            .expect("UART output is infallible");

        if now_us.wrapping_sub(last_toggle_us) >= 500_000 {
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
    }
}
