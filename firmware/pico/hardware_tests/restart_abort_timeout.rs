#![no_std]
#![no_main]

//! Manual hardware test: recovery after a forced DMA-abort timeout.
//!
//! Observe unloaded GP0/GP1 against Pico GND.
//! UART1: GP20 -> TTL RXD, common GND, 3.3 V, 115200 baud, 8N1.
//!
//! Five seconds of initial pulses, then injected fault with active DMA.
//! A zero polling budget forces DmaAbortTimeout after requesting abort.
//! Output must remain Low and faulted for another two seconds.
//! A normal restart then produces GP0 2 ms / GP1 1 ms.
//!
//! This tests the timeout return path, not a physically stuck DMA channel.

mod support;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_pio::{
    OutputBuffer, OutputRestartError, OutputSubmissionError, PreparedOutput,
    SwitchingOutput,
};
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

#[derive(Clone, Copy)]
enum Phase {
    Running,
    CheckingInjectedFault,
    Faulted,
    Restarted,
}

fn signal_plan(
    second_gpio: u8,
    first_pulse_us: u32,
    second_pulse_us: u32,
    period_us: u32,
) -> SignalPlan<3> {
    SignalPlan::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: first_pulse_us,
            },
            PulseTarget {
                gpio: second_gpio,
                pulse_width_us: second_pulse_us,
            },
        ],
        &PulseLimits::new(1_000, 2_000).expect("Valid test limits"),
        period_us,
    )
    .expect("Valid test plan")
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

    let initial = signal_plan(1, 1_000, 2_000, 20_000);
    let pending = signal_plan(1, 1_500, 1_500, 20_000);
    let replacement = signal_plan(1, 2_000, 1_000, 20_000);

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

    let mut output = SwitchingOutput::start(
        prepared,
        dma.ch0,
        first_buffer,
        second_buffer,
        &initial,
    )
    .expect("Failed to start output");

    assert_eq!(
        output.restart(&replacement),
        Err(OutputRestartError::NotFaulted),
    );

    logger
        .log(
            LogLevel::Info,
            "Initial signal plan running",
        )
        .expect("Test message must fit");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut led_high = false;
    let mut phase = Phase::Running;
    let mut phase_started_us = board.timer.get_counter_low();
    let mut last_toggle_us = phase_started_us;

    loop {
        let now_us = board.timer.get_counter_low();

        match phase {
            Phase::Running => {
                output.service();

                if now_us.wrapping_sub(phase_started_us) >= 5_000_000 {
                    output
                        .submit_plan(&pending)
                        .expect("Pending plan must be accepted");

                    assert_eq!(
                        output.submit_plan(&initial),
                        Err(OutputSubmissionError::Busy),
                    );

                    output.force_fault_with_active_dma();

                    assert!(output.is_faulted());
                    assert!(output.dma_is_active());

                    logger
                        .log(
                            LogLevel::Info,
                            "PIO stopped; FIFO full; DMA active",
                        )
                        .expect("Test message must fit");

                    phase = Phase::CheckingInjectedFault;
                    phase_started_us = now_us;
                }
            }
            Phase::CheckingInjectedFault => {
                assert!(output.is_faulted());
                assert!(output.dma_is_active());
                assert!(output.has_dma_transfer());

                assert_eq!(
                    output.restart_with_zero_abort_budget(&initial),
                    Err(OutputRestartError::DmaAbortTimeout),
                );

                assert!(output.is_faulted());
                assert!(output.has_dma_transfer());

                assert_eq!(
                    output.submit_plan(&replacement),
                    Err(OutputSubmissionError::Faulted),
                );

                // The hardware abort may already have completed.
                // Transfer ownership must nevertheless remain intact.
                logger
                    .log(
                        LogLevel::Error,
                        "Abort timeout injected; output remains faulted",
                    )
                    .expect("Test message must fit");

                phase = Phase::Faulted;
                phase_started_us = now_us;
            }
            Phase::Faulted => {
                // Neither servicing nor elapsed time may recover output.
                output.service();

                assert!(output.is_faulted());
                assert!(output.has_dma_transfer());

                if now_us.wrapping_sub(phase_started_us) >= 2_000_000 {
                    assert_eq!(
                        output.submit_plan(&replacement),
                        Err(OutputSubmissionError::Faulted),
                    );

                    output
                        .restart(&replacement)
                        .expect("Retry after timeout must succeed");

                    assert!(!output.is_faulted());
                    assert!(output.has_dma_transfer());

                    assert_eq!(
                        output.restart(&initial),
                        Err(OutputRestartError::NotFaulted),
                    );

                    logger
                        .log(
                            LogLevel::Info,
                            "Restart after abort timeout succeeded",
                        )
                        .expect("Test message must fit");

                    phase = Phase::Restarted;
                }
            }
            Phase::Restarted => {
                output.service();
                assert!(
                    !output.is_faulted(),
                    "Output faulted after restart",
                );
            }
        }

        logger
            .service(16)
            .expect("UART output is infallible");

        let blink_interval_us = match phase {
            Phase::CheckingInjectedFault | Phase::Faulted => 100_000,
            _ => 500_000,
        };

        if now_us.wrapping_sub(last_toggle_us) >= blink_interval_us {
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
