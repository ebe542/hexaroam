#![no_std]
#![no_main]

//! Manual hardware test: read the reserved calibration journal.
//!
//! UART1: GP20 -> adapter TTL RXD, shared GND, 3.3-V logic.
//! Terminal: 115200 baud, 8N1, no flow control.
//! No servo output, flash programming or flash erase is performed.
//!
//! Expected: one result message. LED high on successful reading,
//! including an absent valid record; LED low on error.
//! LED high is a test result, not authorization for robot operation.

mod support;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

// This read-only test intentionally leaves write operations unused.
#[allow(dead_code)]
#[path = "../src/flash_journal_storage.rs"]
mod flash_journal_storage;

#[path = "../src/flash_xip.rs"]
mod flash_xip;

use embedded_hal::digital::OutputPin;
use flash_journal_storage::{FlashJournalStorage, FlashStorageError};
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_storage::{
    JournalRead, SLOT_COUNT, SLOT_SIZE, SoftwareCrc32, load,
};
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::{
    clocks::Clock,
    gpio::FunctionUart,
    uart::{Pins, UartConfig, UartPeripheral},
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

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(uart));
    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    // SAFETY:
    // This standalone Pico test preserves the normal XIP configuration.
    // It starts neither core1 nor DMA and performs no flash writes/erases.
    // No independent flash writer or FlashJournalReader exists.
    let storage = unsafe { FlashJournalStorage::new() };

    let (level, message, success) = match storage {
        Err(_) => (
            LogLevel::Error,
            "Invalid journal linker layout",
            false,
        ),
        Ok(mut storage) => {
            let mut buffer = [0; SLOT_SIZE];

            let invalid_index_rejected = storage
                .read_slot(SLOT_COUNT, &mut buffer)
                == Err(FlashStorageError::UnknownSlot(
                    SLOT_COUNT,
                ));

            if !invalid_index_rejected {
                (
                    LogLevel::Error,
                    "Journal slot bounds check failed",
                    false,
                )
            } else {
                match load::<18, _>(&mut storage, &mut SoftwareCrc32) {
                    Ok(None) => (
                        LogLevel::Info,
                        "Journal read OK: no valid record",
                        true,
                    ),
                    Ok(Some(_)) => (
                        LogLevel::Info,
                        "Journal read OK: valid record found",
                        true,
                    ),
                    Err(_) => (
                        LogLevel::Error,
                        "Journal load failed",
                        false,
                    ),
                }
            }
        }
    };

    if success {
        led.set_high()
            .expect("Failed to turn LED on");
    }

    logger
        .log(level, message)
        .expect("Result message must fit");

    loop {
        logger
            .service(16)
            .expect("UART output is infallible");
    }
}
