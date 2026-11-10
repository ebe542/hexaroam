#![no_std]
#![no_main]

//! Manual single-servo test through CalibrationControl and PioPulseBackend.
//!
//! Observe unloaded GP0 against Pico GND. Do not connect a servo.
//! Debug UART1: GP20 TX, 115200 baud, 8N1, LF.
//!
//! Expected:
//! - Two seconds inactive: GP0 Low, LED on.
//! - Three seconds running: 1500 us High every 20 ms, LED blinking.
//! - Three seconds updated: 1700 us High every 20 ms, LED blinking.
//! - Then permanently stopped: GP0 Low, LED on.
//!
//! A rejected 2001 us request must never change the accepted pulse.

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use calibration_station::{
    control::{
        CalibrationControl, ControlError, OUTPUT_ERROR, PULSE_READY, RUNNING,
    },
    pio_pulse_backend::{PERIOD_US, PioPulseBackend},
};
use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_pio::{OutputBuffer, PreparedOutput, SwitchingOutput};
use hexaroam_servo_signal::PulseLimits;
use rp_pico::{
    entry,
    hal::{
        Timer,
        clocks::{Clock, init_clocks_and_plls},
        dma::DMAExt,
        gpio::{FunctionPio0, FunctionUart, PullNone},
        pac,
        pio::PIOExt,
        sio::Sio,
        uart::{Pins, UartConfig, UartPeripheral},
        watchdog::Watchdog,
    },
};
use uart_logging::UartLogOutput;

enum Phase {
    Inactive,
    Running,
    Updated,
    Stopped,
}

#[entry]
fn main() -> ! {
    let mut peripherals =
        pac::Peripherals::take().expect("RP2040 peripherals already taken");

    let mut watchdog = Watchdog::new(peripherals.WATCHDOG);

    let clocks = init_clocks_and_plls(
        rp_pico::XOSC_CRYSTAL_FREQ,
        peripherals.XOSC,
        peripherals.CLOCKS,
        peripherals.PLL_SYS,
        peripherals.PLL_USB,
        &mut peripherals.RESETS,
        &mut watchdog,
    )
    .ok()
    .expect("Clock initialization failed");

    let sio = Sio::new(peripherals.SIO);

    let pins = rp_pico::Pins::new(
        peripherals.IO_BANK0,
        peripherals.PADS_BANK0,
        sio.gpio_bank0,
        &mut peripherals.RESETS,
    );

    let timer = Timer::new(
        peripherals.TIMER,
        &mut peripherals.RESETS,
        &clocks,
    );

    let uart_pins = Pins::default().tx(pins
        .gpio20
        .into_function::<FunctionUart>());

    let uart = UartPeripheral::new(
        peripherals.UART1,
        uart_pins,
        &mut peripherals.RESETS,
    )
    .enable(
        UartConfig::default(),
        clocks.peripheral_clock.freq(),
    )
    .expect("Failed to initialize logging UART");

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(uart));

    let mut led = pins.led.into_push_pull_output();
    led.set_high()
        .expect("Failed to turn LED on");

    let (mut pio, sm0, _, _, _) = peripherals
        .PIO0
        .split(&mut peripherals.RESETS);
    let dma = peripherals
        .DMA
        .split(&mut peripherals.RESETS);

    let output_pins = [pins
        .gpio0
        .into_function::<FunctionPio0>()
        .into_pull_type::<PullNone>()
        .into_dyn_pin()];

    let limits = PulseLimits::new(1_000, 2_000).expect("Valid test limits");

    // Each entry encodes a GPIO mask and a duration: four words total.
    let first_buffer = cortex_m::singleton!(
        : OutputBuffer<4> = OutputBuffer::new()
    )
    .expect("First buffer already allocated");

    let second_buffer = cortex_m::singleton!(
        : OutputBuffer<4> = OutputBuffer::new()
    )
    .expect("Second buffer already allocated");

    // PIO IRQ flag 0 is reserved for output underflow detection.
    pio.clear_irq(1);

    let prepared = PreparedOutput::new(
        &mut pio,
        sm0,
        output_pins,
        clocks.system_clock.freq().to_Hz(),
    )
    .expect("Failed to prepare output");

    let output = SwitchingOutput::new(
        prepared,
        dma.ch0,
        first_buffer,
        second_buffer,
        PERIOD_US,
    )
    .expect("Failed to initialize inactive output");

    assert!(!output.dma_is_active());

    let backend = PioPulseBackend::new(output, limits)
        .expect("Failed to create pulse backend");

    let mut control = CalibrationControl::new(backend, limits);

    assert_eq!(control.status(), 0);
    assert_eq!(
        control.set_enabled(true),
        Err(ControlError::MissingPulse),
    );

    control
        .set_pulse(1_500)
        .expect("Failed to prepare pulse");

    assert_eq!(control.status(), PULSE_READY);

    logger
        .log(
            LogLevel::Info,
            "Controlled output: prepared; inactive",
        )
        .expect("Test message must fit");

    let mut phase = Phase::Inactive;
    let mut phase_started_us = timer.get_counter_low();
    let mut last_toggle_us = phase_started_us;
    let mut led_high = true;

    loop {
        control.service();
        assert_eq!(control.status() & OUTPUT_ERROR, 0);

        let now_us = timer.get_counter_low();

        match phase {
            Phase::Inactive => {
                assert_eq!(control.status(), PULSE_READY);

                if now_us.wrapping_sub(phase_started_us) >= 2_000_000 {
                    control
                        .set_enabled(true)
                        .expect("Failed to start output");

                    assert_eq!(control.status(), PULSE_READY | RUNNING);

                    logger
                        .log(
                            LogLevel::Info,
                            "Controlled output: 1500 us running",
                        )
                        .expect("Test message must fit");

                    phase = Phase::Running;
                    phase_started_us = timer.get_counter_low();
                    last_toggle_us = phase_started_us;
                }
            }
            Phase::Running => {
                assert_eq!(control.status(), PULSE_READY | RUNNING);

                if now_us.wrapping_sub(phase_started_us) >= 3_000_000 {
                    control
                        .set_pulse(1_700)
                        .expect("Failed to submit pulse update");

                    // Reject the invalid request without replacing the
                    // already accepted update, even if it is still pending.
                    assert_eq!(
                        control.set_pulse(2_001),
                        Err(ControlError::OutOfRange),
                    );
                    assert_eq!(control.pulse_width_us(), Some(1_700));

                    logger
                        .log(
                            LogLevel::Info,
                            "Controlled output: 1700 us accepted",
                        )
                        .expect("Test message must fit");

                    phase = Phase::Updated;
                    phase_started_us = timer.get_counter_low();
                }
            }
            Phase::Updated => {
                assert_eq!(control.status(), PULSE_READY | RUNNING);
                assert_eq!(control.pulse_width_us(), Some(1_700));

                if now_us.wrapping_sub(phase_started_us) >= 3_000_000 {
                    control
                        .set_enabled(false)
                        .expect("Failed to stop output");
                    control
                        .set_enabled(false)
                        .expect("Repeated stop must succeed");

                    assert_eq!(control.status(), PULSE_READY);
                    assert_eq!(control.pulse_width_us(), Some(1_700));

                    led.set_high()
                        .expect("Failed to turn LED on");

                    logger
                        .log(
                            LogLevel::Info,
                            "Controlled output OK: stopped",
                        )
                        .expect("Test message must fit");

                    phase = Phase::Stopped;
                }
            }
            Phase::Stopped => {
                assert_eq!(control.status(), PULSE_READY);
            }
        }

        // Refresh after transitions that sampled the timer again.
        let now_us = timer.get_counter_low();

        if matches!(phase, Phase::Running | Phase::Updated)
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
