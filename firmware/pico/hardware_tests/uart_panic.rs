#![no_std]
#![no_main]

//! Manual hardware test: bounded UART1 panic output.
//!
//! Wiring: GP20 -> adapter TTL RXD, GND -> adapter TTL GND.
//! Adapter: 3.3 V logic. Terminal: 115200 baud, 8N1.
//!
//! Expected:
//! - One buffered startup message.
//! - After five seconds, the LED turns on.
//! - A panic message with source location appears.
//! - Normal execution stops.
//!
//! No servo output is started.

mod support;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use rp_pico::{
    entry,
    hal::{
        clocks::Clock,
        gpio::FunctionUart,
        uart::{Pins, UartConfig, UartPeripheral},
    },
};
use uart_logging::UartLogOutput;

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
        .expect("Failed to initialize diagnostic UART");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut logger = Logger::<_, 128>::new(UartLogOutput::new(uart));

    logger
        .log(
            LogLevel::Info,
            "Panic test starts in five seconds",
        )
        .expect("Startup message must fit");

    let started_us = board.timer.get_counter_low();

    loop {
        logger
            .service(16)
            .expect("UART output is infallible");

        if board
            .timer
            .get_counter_low()
            .wrapping_sub(started_us)
            >= 5_000_000
        {
            led.set_high()
                .expect("Failed to turn LED on");

            panic!("Intentional UART panic test");
        }
    }
}
