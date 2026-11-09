#![no_std]
#![no_main]

//! Manual hardware test: regular startup after a confirmed stop.
//!
//! Observe unloaded GP0 and GP1 against Pico GND.
//! UART1: GP20 -> TTL RXD, common GND, 3.3 V, 115200 baud, 8N1.
//!
//! Expected:
//! - Five seconds: GP0 1 ms, GP1 2 ms, LED blinking.
//! - Two seconds: both signals Low, LED continuously on.
//! - Then GP0 2 ms, GP1 1 ms, LED blinking again.
//! - The discarded 1.5 ms replacement must not reappear.
//! - Invalid starts preserve stopped output.
//! - Starting active output is rejected without changing its plan.
//!
//! No servo or mechanical load is required.

mod support;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_pio::{
    OutputBuffer, OutputLifecycleError, OutputSubmissionError, PreparedOutput,
    SwitchingOutput,
};
use hexaroam_servo_signal::{
    PulseLimits, PulseTarget, SignalOutputState, SignalPlan,
};
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

fn signal_plan(first_us: u32, second_us: u32) -> SignalPlan<3> {
    SignalPlan::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: first_us,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: second_us,
            },
        ],
        &PulseLimits::new(1_000, 2_000).unwrap(),
        20_000,
    )
    .expect("Valid test plan")
}

fn incompatible_plan(second_gpio: u8, period_us: u32) -> SignalPlan<3> {
    SignalPlan::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: 1_000,
            },
            PulseTarget {
                gpio: second_gpio,
                pulse_width_us: 2_000,
            },
        ],
        &PulseLimits::new(1_000, 2_000).unwrap(),
        period_us,
    )
    .expect("Structurally valid but incompatible test plan")
}

#[derive(Clone, Copy)]
enum Phase {
    Running,
    Stopped,
    StartedAgain,
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

    let initial = signal_plan(1_000, 2_000);
    let pending = signal_plan(1_500, 1_500);
    let replacement = signal_plan(2_000, 1_000);
    let wrong_period = incompatible_plan(1, 10_000);
    let wrong_outputs = incompatible_plan(2, 20_000);

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
    .expect("Failed to prepare output");

    let mut output = SwitchingOutput::start(
        prepared,
        dma.ch0,
        first_buffer,
        second_buffer,
        &initial,
    )
    .expect("Failed to start output");

    assert_eq!(
        output.state(),
        SignalOutputState::Active
    );

    assert_eq!(
        output.start_plan(&replacement),
        Err(OutputLifecycleError::AlreadyActive),
    );
    assert!(!output.is_faulted());

    logger
        .log(
            LogLevel::Info,
            "Start test: initial plan running",
        )
        .expect("Test message must fit");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut phase = Phase::Running;
    let mut phase_started_us = board.timer.get_counter_low();
    let mut last_toggle_us = phase_started_us;
    let mut led_high = false;

    loop {
        let now_us = board.timer.get_counter_low();

        output.service();

        match phase {
            Phase::Running => {
                assert_eq!(
                    output.state(),
                    SignalOutputState::Active
                );
                assert!(!output.is_faulted());

                if now_us.wrapping_sub(phase_started_us) >= 5_000_000 {
                    // No service call between submission and stop.
                    output
                        .submit_plan(&pending)
                        .expect("Pending replacement must be accepted");

                    output
                        .stop()
                        .expect("Stop must succeed");

                    assert_eq!(
                        output.state(),
                        SignalOutputState::Inactive,
                    );
                    assert!(!output.has_dma_transfer());
                    assert!(!output.is_faulted());

                    assert_eq!(
                        output.start_plan(&wrong_period),
                        Err(OutputLifecycleError::PeriodMismatch),
                    );
                    assert_eq!(
                        output.state(),
                        SignalOutputState::Inactive,
                    );
                    assert!(!output.has_dma_transfer());

                    assert_eq!(
                        output.start_plan(&wrong_outputs),
                        Err(OutputLifecycleError::OutputMismatch),
                    );
                    assert_eq!(
                        output.state(),
                        SignalOutputState::Inactive,
                    );
                    assert!(!output.has_dma_transfer());
                    assert!(!output.is_faulted());

                    assert_eq!(
                        output.submit_plan(&initial),
                        Err(OutputSubmissionError::Stopped),
                    );

                    led.set_high()
                        .expect("Failed to turn LED on");

                    logger
                        .log(
                            LogLevel::Info,
                            "Start test: stopped; invalid starts rejected",
                        )
                        .expect("Test message must fit");

                    phase = Phase::Stopped;
                    phase_started_us = board.timer.get_counter_low();
                }
            }
            Phase::Stopped => {
                assert_eq!(
                    output.state(),
                    SignalOutputState::Inactive,
                );
                assert!(!output.has_dma_transfer());
                assert!(!output.is_faulted());

                if now_us.wrapping_sub(phase_started_us) >= 2_000_000 {
                    output
                        .start_plan(&replacement)
                        .expect("Regular startup must succeed");

                    assert_eq!(
                        output.state(),
                        SignalOutputState::Active,
                    );
                    assert!(output.has_dma_transfer());
                    assert!(!output.is_faulted());

                    // Reject another startup without replacing the new plan.
                    assert_eq!(
                        output.start_plan(&initial),
                        Err(OutputLifecycleError::AlreadyActive),
                    );

                    logger
                        .log(
                            LogLevel::Info,
                            "Start test OK: replacement plan running",
                        )
                        .expect("Test message must fit");

                    led.set_low()
                        .expect("Failed to turn LED off");
                    led_high = false;
                    last_toggle_us = board.timer.get_counter_low();
                    phase = Phase::StartedAgain;
                }
            }
            Phase::StartedAgain => {
                assert_eq!(
                    output.state(),
                    SignalOutputState::Active
                );
                assert!(!output.is_faulted());
            }
        }

        // Phase transitions may have sampled the timer again.
        // Refresh now so it cannot precede last_toggle_us.
        let now_us = board.timer.get_counter_low();
        if !matches!(phase, Phase::Stopped)
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
