#![no_std]
#![no_main]

//! Manual hardware test: persist a complete calibration journal record.
//!
//! Without a valid record: append a synthetic fixture and verify loading.
//! With the expected record: verify without writing again.
//! Unexpected valid records are rejected without modification.
//! Invalid/occupied slots are handled by the existing append policy.
//!
//! Synthetic data is for storage testing, not robot operation.
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
use flash_journal_storage::FlashJournalStorage;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_control::{Angle, Calibration};
use hexaroam_servo_storage::{SoftwareCrc32, append, load};
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::{
    clocks::Clock,
    gpio::FunctionUart,
    uart::{Pins, UartConfig, UartPeripheral},
};
use uart_logging::UartLogOutput;

const SERVO_COUNT: usize = 18;
const EXPECTED_SEQUENCE: u64 = 1;

fn calibration_fixture()
-> Result<[Option<Calibration>; SERVO_COUNT], &'static str> {
    let mut calibrations = [None; SERVO_COUNT];

    for (index, entry) in calibrations.iter_mut().enumerate() {
        // Distinct values help detect servo-ID or ordering mistakes.
        let angle_offset = index as i32 * 10;
        let pulse_offset = index as u32 * 5;

        let calibration = Calibration::new(
            Angle::from_centidegrees(-4_500 + angle_offset),
            1_000 + pulse_offset,
            Angle::from_centidegrees(4_500 + angle_offset),
            2_000 - pulse_offset,
        )
        .map_err(|_| "Invalid synthetic calibration fixture")?;

        *entry = Some(calibration);
    }

    Ok(calibrations)
}

fn run_test() -> Result<&'static str, &'static str> {
    let expected = calibration_fixture()?;

    // SAFETY:
    // This standalone Pico test preserves the standard XIP configuration.
    // It starts neither core1 nor DMA and creates no other flash backend.
    // No independent flash writer or FlashJournalReader exists.
    // No unmasked exception or debugger operation may access flash
    // during backend programming/erase operations.
    let mut storage = unsafe { FlashJournalStorage::new() }
        .map_err(|_| "Invalid journal linker layout")?;

    let mut checksum = SoftwareCrc32;

    let existing = load::<SERVO_COUNT, _>(&mut storage, &mut checksum)
        .map_err(|_| "Initial journal load failed")?;

    if let Some(existing) = existing {
        if existing.record.sequence != EXPECTED_SEQUENCE
            || existing.record.calibrations != expected
        {
            return Err("Unexpected valid record; journal unchanged");
        }

        // No append or erase operation occurs in this branch.
        return Ok("Journal persistence OK: existing record");
    }

    let written = append(&mut storage, &expected, &mut checksum)
        .map_err(|_| "Journal append failed")?;

    if written.record.sequence != EXPECTED_SEQUENCE
        || written.record.calibrations != expected
    {
        return Err("Append returned unexpected record");
    }

    // Reload through the complete decode/CRC/selection path.
    let loaded = load::<SERVO_COUNT, _>(&mut storage, &mut checksum)
        .map_err(|_| "Journal reload failed")?
        .ok_or("Written journal record not found")?;

    if loaded.slot_index != written.slot_index
        || loaded.record != written.record
    {
        return Err("Reloaded journal record differs");
    }

    Ok("Journal persistence OK: record written")
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
        Ok(message) => {
            led.set_high()
                .expect("Failed to turn LED on");
            (LogLevel::Info, message)
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
