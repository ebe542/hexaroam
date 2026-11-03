#![no_std]
#![no_main]

//! Manual register-protocol test over UART0 without servo output.
//!
//! Remove the GP16-GP17 loopback jumper.
//! Connect a 3.3 V TTL adapter:
//! - GP16 / UART0 TX -> adapter RXD
//! - GP17 / UART0 RX <- adapter TXD
//! - Pico GND       -> adapter GND
//!
//! Protocol UART: 115200 baud, 8N1, no flow control.
//! Debug UART1 remains on GP20, separate from protocol traffic.
//!
//! Expected register values:
//! - ProtocolVersion: 1
//! - Status: 0, no calibration and no active output
//! - ServoCount: 0
//!
//! Only READ requests produce protocol replies.
//! LED blinking indicates that the main loop is running.

mod support;

#[path = "support/no_servo_output.rs"]
mod no_servo_output;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

#[path = "../src/uart_transport.rs"]
mod uart_transport;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_registers::{
    FrameProcessingError, FrameProcessor, RegisterService,
};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};
use no_servo_output::NoServoOutput;
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

// Initial manual-test value, not a production timing guarantee.
const RECEIVE_IDLE_TIMEOUT_US: u32 = 100_000;
const RECEIVE_BUDGET: usize = 32;
const TRANSMIT_BUDGET: usize = 32;

fn checksum(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
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

    let registers = RegisterService::new(2, 0, NoServoOutput, false);
    let mut processor =
        FrameProcessor::new(registers).expect("Valid register map");

    logger
        .log(
            LogLevel::Info,
            "UART register test ready",
        )
        .expect("Test message must fit");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut led_high = false;
    let mut last_toggle_us = board.timer.get_counter_low();
    let mut last_receive_us = last_toggle_us;
    let mut receive_timeout_armed = false;

    loop {
        match processor.poll(
            &mut transport,
            RECEIVE_BUDGET,
            TRANSMIT_BUDGET,
            &mut checksum,
        ) {
            Ok(progress) => {
                let now_us = board.timer.get_counter_low();

                if progress.received != 0 {
                    last_receive_us = now_us;
                    receive_timeout_armed = true;
                }

                // Only declare idle after reception actually found no
                // pending transport data. Budget exhaustion alone is
                // not evidence of an idle UART.
                if progress.receive_empty
                    && receive_timeout_armed
                    && now_us.wrapping_sub(last_receive_us)
                        >= RECEIVE_IDLE_TIMEOUT_US
                {
                    processor.on_receive_timeout();
                    receive_timeout_armed = false;
                }
            }
            Err(FrameProcessingError::Receive(_)) => {
                // The UART has discarded an erroneous receive entry.
                // Preserve complete requests, but invalidate any remaining
                // incomplete frame through the processor's deferred reset.
                processor.on_receive_timeout();
                receive_timeout_armed = false;

                // Diagnostic messages may be dropped if their queue is full.
                let _ = logger.log(
                    LogLevel::Error,
                    "Protocol UART receive error",
                );
            }
            Err(error) => {
                panic!(
                    "Protocol processing failed: {:?}",
                    error
                );
            }
        }

        logger
            .service(16)
            .expect("Debug UART output is infallible");

        let now_us = board.timer.get_counter_low();

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
