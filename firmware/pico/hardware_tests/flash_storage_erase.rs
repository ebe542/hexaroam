#![no_std]
#![no_main]

//! Manual hardware test: erase through the JournalStorage backend.
//!
//! Destroys all contents of journal sector 0.
//! If already erased, writes a test pattern before testing erase.
//! Verifies rejected indices, erased contents and unchanged sector 1.
//! Also checks successful repeated erasure of an already erased sector.
//! No core1, DMA or servo output is started.
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
use hexaroam_servo_storage::{
    JournalRead, JournalStorage, SECTOR_COUNT, SLOT_COUNT, SLOT_SIZE,
    SLOTS_PER_SECTOR,
};
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::{
    clocks::Clock,
    gpio::FunctionUart,
    uart::{Pins, UartConfig, UartPeripheral},
};
use uart_logging::UartLogOutput;

fn verify_sectors(
    storage: &mut FlashJournalStorage,
    before: &[[u8; SLOT_SIZE]; SLOT_COUNT],
) -> Result<(), &'static str> {
    let mut actual = [0_u8; SLOT_SIZE];

    for (index, original) in before.iter().enumerate() {
        storage
            .read_slot(index, &mut actual)
            .map_err(|_| "Sector verification read failed")?;

        if index < SLOTS_PER_SECTOR {
            if actual.iter().any(|&byte| byte != 0xFF) {
                return Err("Sector 0 is not completely erased");
            }
        } else if actual != *original {
            return Err("Sector 1 changed unexpectedly");
        }
    }

    Ok(())
}

fn run_test() -> Result<(), &'static str> {
    // SAFETY:
    // This standalone Pico test preserves the standard XIP configuration.
    // It starts neither core1 nor DMA and creates no other flash backend.
    // No independent flash writer or FlashJournalReader exists.
    // No unmasked exception or debugger operation may access flash
    // during backend programming/erase operations.
    let mut storage = unsafe { FlashJournalStorage::new() }
        .map_err(|_| "Invalid journal linker layout")?;

    // Snapshot both sectors before checking rejected operations.
    let mut before = [[0_u8; SLOT_SIZE]; SLOT_COUNT];
    for (index, slot) in before.iter_mut().enumerate() {
        storage
            .read_slot(index, slot)
            .map_err(|_| "Initial journal read failed")?;
    }

    for invalid_index in [SECTOR_COUNT, usize::MAX] {
        if storage.erase_sector(invalid_index)
            != Err(FlashStorageError::UnknownSector(
                invalid_index,
            ))
        {
            return Err("Invalid sector index was not rejected");
        }
    }

    let mut actual = [0_u8; SLOT_SIZE];
    for (index, original) in before.iter().enumerate() {
        storage
            .read_slot(index, &mut actual)
            .map_err(|_| "Read after invalid erase failed")?;

        if actual != *original {
            return Err("Invalid erase changed the journal");
        }
    }

    let target_erased = before[..SLOTS_PER_SECTOR]
        .iter()
        .all(|slot| slot.iter().all(|&byte| byte == 0xFF));

    if target_erased {
        // Ensure the next erase must actually remove programmed bits.
        let pattern = [0x3C_u8; SLOT_SIZE];

        storage
            .write_slot(0, &pattern)
            .map_err(|_| "Erase test preparation failed")?;

        storage
            .read_slot(0, &mut actual)
            .map_err(|_| "Preparation read failed")?;

        if actual != pattern {
            return Err("Preparation pattern differs");
        }
    }

    // Sector 0 is deliberately expendable in this hardware test.
    storage
        .erase_sector(0)
        .map_err(|_| "Backend sector erase failed")?;

    verify_sectors(&mut storage, &before)?;

    // The already-erased case must also succeed.
    storage
        .erase_sector(0)
        .map_err(|_| "Already erased sector was rejected")?;

    verify_sectors(&mut storage, &before)?;

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
            (LogLevel::Info, "Flash storage erase OK")
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
