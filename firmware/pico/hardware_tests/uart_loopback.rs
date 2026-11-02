#![no_std]
#![no_main]

//! Manual UART0 byte-transport loopback test.
//!
//! Connect GP16 directly to GP17 before starting.
//! UART0: TX GP16, RX GP17, 115200 baud, 8N1.
//!
//! Debug UART1: GP20 -> TTL RXD, common GND,
//! 3.3 V TTL receiver, 115200 baud, 8N1.
//!
//! Sends and verifies 1024 bytes, covering all byte values four times.
//! Success: LED on and INFO UART loopback OK.
//!
//! This does not test framing, register commands or servo output.

mod support;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

#[path = "../src/uart_transport.rs"]
mod uart_transport;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_protocol::ByteTransport;
use rp_pico::{
    entry,
    hal::{
        clocks::Clock,
        gpio::FunctionUart,
        uart::{Pins, UartConfig, UartPeripheral},
    },
};
use uart_logging::UartLogOutput;
use uart_transport::UartTransport;

const TEST_BYTES: usize = 1024;
const TEST_TIMEOUT_US: u32 = 2_000_000;

fn test_byte(index: usize) -> u8 {
    (index & 0xFF) as u8
}

#[entry]
fn main() -> ! {
    let mut board = support::initialize();

    let debug_pins = Pins::default().tx(board
        .pins
        .gpio20
        .into_function::<FunctionUart>());

    let debug_uart = UartPeripheral::new(
        board.uart,
        debug_pins,
        &mut board.resets,
    )
    .enable(
        UartConfig::default(),
        board.peripheral_clock.freq(),
    )
    .expect("Failed to initialize debug UART");

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(debug_uart));

    let protocol_pins = Pins::default()
        .tx(board
            .pins
            .gpio16
            .into_function::<FunctionUart>())
        .rx(board
            .pins
            .gpio17
            .into_function::<FunctionUart>());

    let protocol_uart = UartPeripheral::new(
        board.protocol_uart,
        protocol_pins,
        &mut board.resets,
    )
    .enable(
        UartConfig::default(),
        board.peripheral_clock.freq(),
    )
    .expect("Failed to initialize protocol UART");

    let mut transport = UartTransport::new(protocol_uart);

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    logger
        .log(LogLevel::Info, "UART loopback started")
        .expect("Test message must fit");

    let mut sent = 0;
    let mut received = 0;
    let started_us = board.timer.get_counter_low();

    while received < TEST_BYTES {
        // At most one transmit attempt per iteration.
        // A full FIFO does not block receive or debug servicing.
        if sent < TEST_BYTES
            && transport
                .try_write(test_byte(sent))
                .expect("UART loopback transmit failed")
        {
            sent += 1;
        }

        if let Some(byte) = transport
            .try_read()
            .expect("UART loopback receive failed")
        {
            assert!(
                received < sent,
                "Received data before its transmit acceptance",
            );
            assert_eq!(
                byte,
                test_byte(received),
                "UART loopback byte mismatch",
            );

            received += 1;
        }

        logger
            .service(16)
            .expect("Debug UART output is infallible");

        assert!(
            board
                .timer
                .get_counter_low()
                .wrapping_sub(started_us)
                < TEST_TIMEOUT_US,
            "UART loopback timed out; check GP16-GP17 jumper",
        );
    }

    assert_eq!(sent, TEST_BYTES);

    led.set_high()
        .expect("Failed to turn LED on");

    logger
        .log(LogLevel::Info, "UART loopback OK")
        .expect("Test message must fit");

    loop {
        logger
            .service(16)
            .expect("Debug UART output is infallible");
    }
}
