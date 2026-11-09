//! Fixed two-channel signal test using one PIO state machine and DMA.

use crate::uart_logging::UartLogOutput;
use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_pio::{OutputBuffer, PreparedOutput};
use hexaroam_servo_signal::{PulseLimits, PulseTarget, SignalPlan};
use rp_pico::hal::{
    Timer,
    clocks::{Clock, PeripheralClock},
    dma::DMAExt,
    gpio::{FunctionPio0, FunctionUart, PullNone},
    pac,
    pio::PIOExt,
    uart::{Pins, UartConfig, UartPeripheral},
};

pub fn run(
    pio_peripheral: pac::PIO0,
    dma_peripheral: pac::DMA,
    pins: rp_pico::Pins,
    resets: &mut pac::RESETS,
    system_clock_hz: u32,
    timer: Timer,
    uart_peripheral: pac::UART1,
    peripheral_clock: PeripheralClock,
) -> ! {
    let uart_pins = Pins::default().tx(pins
        .gpio20
        .into_function::<FunctionUart>());

    let uart = UartPeripheral::new(uart_peripheral, uart_pins, resets)
        .enable(
            UartConfig::default(),
            peripheral_clock.freq(),
        )
        .expect("Failed to initialize logging UART");

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(uart));

    logger
        .log(
            LogLevel::Info,
            "Initializing servo output",
        )
        .expect("Startup message must fit");
    let (mut pio, sm0, _, _, _) = pio_peripheral.split(resets);
    let dma = dma_peripheral.split(resets);

    let output_pins = [
        pins.gpio0
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
        pins.gpio1
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
    ];

    let limits = PulseLimits::new(1_000, 2_000).expect("Invalid test limits");

    let plan = SignalPlan::<3>::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: 1_000,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: 2_000,
            },
        ],
        &limits,
        20_000,
    )
    .expect("Failed to construct test plan");

    // Allocate once in static RAM. The driver freezes the payload
    // before handing a static read-only slice to DMA.
    let buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("Output buffer already allocated");

    // IRQ flag 0 is reserved for this output's underflow indication.
    pio.clear_irq(1);

    let prepared = PreparedOutput::new(
        &mut pio,
        sm0,
        output_pins,
        system_clock_hz,
    )
    .expect("Failed to prepare servo outputs");

    let mut output = prepared
        .start_fixed(dma.ch0, buffer, &plan)
        .expect("Failed to start fixed output");

    let mut led = pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut led_high = false;
    let mut last_toggle_us = timer.get_counter_low();

    logger
        .log(LogLevel::Info, "Servo output started")
        .expect("Startup message must fit");

    let mut last_log_us = timer.get_counter_low();
    loop {
        output.service();
        logger
            .service(16)
            .expect("UART output is infallible");

        let now_us = timer.get_counter_low();

        if now_us.wrapping_sub(last_log_us) >= 1_000_000 {
            last_log_us = now_us;

            // Runtime messages may be dropped when the buffer is full.
            let _ = logger.log(LogLevel::Debug, "Servo demo alive");
        }

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
