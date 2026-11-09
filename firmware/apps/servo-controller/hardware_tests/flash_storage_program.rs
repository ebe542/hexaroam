#![no_std]
#![no_main]

//! Manual hardware test: program a slot through FlashJournalStorage.
//!
//! Requires erased slot 0. Writes a test pattern, not calibration data.
//! Checks rejected requests and verifies that they preserve slot 0.
//! No erase, core1, DMA or servo output is started.
//!
//! UART1: GP20 -> adapter TTL RXD, shared GND, 3.3-V logic.
//! Terminal: 115200 baud, 8N1, no flow control.

mod support;

#[path = "../src/flash_journal_storage.rs"]
mod flash_journal_storage;

#[path = "../src/flash_xip.rs"]
mod flash_xip;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use embedded_hal::digital::OutputPin;
use flash_journal_storage::{FlashJournalStorage, FlashStorageError};
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_storage::{JournalRead, SLOT_COUNT, SLOT_SIZE};
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::{
    clocks::Clock,
    gpio::FunctionUart,
    uart::{Pins, UartConfig, UartPeripheral},
};
use uart_logging::UartLogOutput;

// Immutable static data is expected in flash. Check its placement below.
static FLASH_DATA: [u8; SLOT_SIZE] = [0x5A; SLOT_SIZE];

fn run_test() -> Result<(), &'static str> {
    let flash_source = FLASH_DATA.as_ptr() as usize;

    if flash_source < 0x1000_0000
        || flash_source
            .checked_add(SLOT_SIZE)
            .is_none_or(|end| end > 0x1020_0000)
    {
        return Err("Test source is not in mapped flash");
    }

    // SAFETY:
    // This standalone Pico test preserves the standard XIP configuration.
    // It starts neither core1 nor DMA and creates no other flash backend.
    // No independent flash writer or FlashJournalReader exists.
    // No unmasked exception or debugger operation may access flash
    // during the backend's programming operation.
    let mut storage = unsafe { FlashJournalStorage::new() }
        .map_err(|_| "Invalid journal linker layout")?;

    let mut before = [0_u8; SLOT_SIZE];
    storage
        .read_slot(0, &mut before)
        .map_err(|_| "Initial slot read failed")?;

    let mut data = [0_u8; SLOT_SIZE];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = (index as u8) ^ 0x3C;
    }

    // The index must be rejected before any programming occurs.
    if storage.program_slot(SLOT_COUNT, &data)
        != Err(FlashStorageError::UnknownSlot(
            SLOT_COUNT,
        ))
    {
        return Err("Invalid slot index was not rejected");
    }

    // Valid index, but the source must not depend on XIP.
    if storage.program_slot(0, &FLASH_DATA)
        != Err(FlashStorageError::SourceNotInRam)
    {
        return Err("Flash source was not rejected");
    }

    let mut actual = [0_u8; SLOT_SIZE];
    storage
        .read_slot(0, &mut actual)
        .map_err(|_| "Read after rejected requests failed")?;

    if actual != before {
        return Err("Rejected request changed slot 0");
    }

    // Existing contents must never be overwritten by this test.
    if before.iter().any(|&byte| byte != 0xFF) {
        if storage.program_slot(0, &data)
            != Err(FlashStorageError::SlotNotErased)
        {
            return Err("Occupied slot was not rejected");
        }

        storage
            .read_slot(0, &mut actual)
            .map_err(|_| "Occupied slot read failed")?;

        if actual != before {
            return Err("Occupied slot changed unexpectedly");
        }

        return Err("Slot 0 occupied; run erase preparation");
    }

    storage
        .program_slot(0, &data)
        .map_err(|_| "Backend slot programming failed")?;

    // Independent check through the backend's JournalRead implementation.
    storage
        .read_slot(0, &mut actual)
        .map_err(|_| "Programmed slot read failed")?;

    if actual != data {
        return Err("Programmed slot contents differ");
    }

    // Try different data so unintended acceptance cannot go unnoticed.
    data[0] ^= 0xFF;

    if storage.program_slot(0, &data) != Err(FlashStorageError::SlotNotErased) {
        return Err("Repeat programming was not rejected");
    }

    // actual still contains the successfully programmed original pattern.
    storage
        .read_slot(0, &mut before)
        .map_err(|_| "Read after repeat rejection failed")?;

    if before != actual {
        return Err("Repeat programming changed slot 0");
    }

    Ok(())
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
        .expect("Failed to initialize diagnostic UART");

    let mut logger = Logger::<_, 128>::new(UartLogOutput::new(uart));
    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let (level, message) = match run_test() {
        Ok(()) => {
            led.set_high()
                .expect("Failed to turn LED on");
            (
                LogLevel::Info,
                "Flash storage program OK",
            )
        }
        Err(message) => (LogLevel::Error, message),
    };

    logger
        .log(level, message)
        .expect("Result message must fit");

    loop {
        logger
            .service(16)
            .expect("UART output is infallible");
    }
}
