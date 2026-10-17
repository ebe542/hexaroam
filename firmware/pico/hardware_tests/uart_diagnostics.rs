#![no_std]
#![no_main]

//! Manual hardware test: UART1 diagnostic output.
//!
//! Wiring:
//! - Pico GP20 -> adapter TTL RXD
//! - Pico GND  -> adapter TTL GND
//! - Adapter logic level: 3.3 V
//!
//! Terminal: 115200 baud, 8 data bits, no parity,
//! one stop bit, no flow control.
//!
//! Expected: startup message and one alive message per second.
//! The onboard LED changes state with every alive message.
//! No servo output is started.

mod support;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::{
    clocks::Clock,
    gpio::FunctionUart,
    uart::{Pins, UartConfig, UartPeripheral},
};
use uart_logging::UartLogOutput;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

#[entry]
fn main() -> ! {
    let mut board = support::initialize();

    let uart_pins = Pins::default().tx(board
        .pins
        .gpio20
        .into_function::<FunctionUart>());

    let uart = UartPeripheral::new(board.uart, uart_pins, &mut board.resets)
        .enable(
            // HAL default: 115200 baud, eight data bits,
            // no parity, one stop bit.
            UartConfig::default(),
            board.peripheral_clock.freq(),
        )
        .expect("Failed to initialize diagnostic UART");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut logger = Logger::<_, 128>::new(UartLogOutput::new(uart));

    logger
        .log(LogLevel::Info, "UART1 logging ready")
        .expect("Startup message must fit");

    logger
        .log(
            LogLevel::Debug,
            "Buffered output enabled",
        )
        .expect("Startup message must fit");

    logger
        .log(LogLevel::Error, "Test message")
        .expect("Startup message must fit");

    let mut led_high = false;
    let mut last_message_us = board.timer.get_counter_low();

    loop {
        logger
            .service(16)
            .expect("UART output is infallible");

        let now_us = board.timer.get_counter_low();

        if now_us.wrapping_sub(last_message_us) >= 1_000_000 {
            last_message_us = now_us;

            // Dropping a complete message is permitted when the buffer is full.
            let _ = logger.log(LogLevel::Info, "Pico alive");

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
