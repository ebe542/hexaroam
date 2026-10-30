#![no_std]
#![no_main]

//! Manual hardware test: rotate the two-sector calibration journal.
//!
//! Preparation erases both sectors, accepting only an empty/invalid
//! journal, the known sequence-1 fixture or the sequence-26 marker.
//! Writes sequences 1 through 25 and checks every append/reload.
//! Checks preservation of the previous sector during each switch.
//! Restart at sequence 25 verifies it and appends completion marker 26.
//! Restart at marker 26 prepares and repeats the complete rotation test.
//! Unexpected valid records are rejected without modification.
//!
//! Synthetic data only; no core1, DMA or servo output is started.
//! UART1: GP20 -> adapter TTL RXD, shared GND, 3.3-V logic.
//! Terminal: 115200 baud, 8N1, no flow control.

mod support;

#[path = "../src/flash_journal_storage.rs"]
mod flash_journal_storage;

#[path = "../src/flash_xip.rs"]
mod flash_xip;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use core::fmt::{self, Write};
use embedded_hal::digital::OutputPin;
use flash_journal_storage::FlashJournalStorage;
use hexaroam_logging::{ByteOutput, LogLevel, Logger};
use hexaroam_servo_control::{Angle, Calibration};
use hexaroam_servo_storage::{
    JournalRead, JournalStorage, SECTOR_COUNT, SLOT_COUNT, SLOT_SIZE,
    SLOTS_PER_SECTOR, SoftwareCrc32, append, load,
};
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::{
    clocks::Clock,
    gpio::FunctionUart,
    uart::{Pins, UartConfig, UartPeripheral},
};
use uart_logging::UartLogOutput;

const SERVO_COUNT: usize = 18;
const FINAL_SEQUENCE: u64 = 25;
// A separate durable marker makes repeat runs automatic without a build flag.
// It is appended on the boot AFTER the full rotation, then permits preparation
// on the following boot. This tests persistence before destroying the fixture.
const COMPLETION_SEQUENCE: u64 = FINAL_SEQUENCE + 1;
// Enough for this test's complete bounded progress output,
// even if UART servicing temporarily cannot transmit bytes.
const LOG_CAPACITY: usize = 4096;

fn calibration_fixture(
    sequence: u64,
) -> Result<[Option<Calibration>; SERVO_COUNT], &'static str> {
    if !(1..=COMPLETION_SEQUENCE).contains(&sequence) {
        return Err("Invalid fixture sequence");
    }

    let mut calibrations = [None; SERVO_COUNT];
    let sequence_offset = (sequence - 1) as u32;

    for (index, entry) in calibrations.iter_mut().enumerate() {
        let angle_offset = index as i32 * 10;
        let pulse_offset = index as u32 * 5;

        // Sequence 1 matches the previous persistence-test fixture.
        // Later records have distinct values.
        *entry = Some(
            Calibration::new(
                Angle::from_centidegrees(-4_500 + angle_offset),
                1_000 + pulse_offset + sequence_offset,
                Angle::from_centidegrees(4_500 + angle_offset),
                2_000 - pulse_offset,
            )
            .map_err(|_| "Invalid synthetic calibration fixture")?,
        );
    }

    Ok(calibrations)
}

struct ProgressText {
    bytes: [u8; 80],
    len: usize,
}

impl Write for ProgressText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self
            .len
            .checked_add(text.len())
            .ok_or(fmt::Error)?;
        let destination = self
            .bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?;

        destination.copy_from_slice(text.as_bytes());
        self.len = end;

        Ok(())
    }
}

fn log_progress<O: ByteOutput>(
    logger: &mut Logger<O, LOG_CAPACITY>,
    phase: &str,
    sequence: u64,
    slot_index: usize,
) -> Result<(), &'static str> {
    let mut text = ProgressText {
        bytes: [0; 80],
        len: 0,
    };

    write!(
        &mut text,
        "Journal {phase}: seq={sequence} sector={} slot={}",
        slot_index / SLOTS_PER_SECTOR,
        slot_index % SLOTS_PER_SECTOR,
    )
    .map_err(|_| "Progress message formatting failed")?;

    let message = core::str::from_utf8(&text.bytes[..text.len])
        .map_err(|_| "Invalid progress message encoding")?;

    logger
        .log(LogLevel::Info, message)
        .map_err(|_| "Progress log buffer full")?;

    logger
        .service(128)
        .map_err(|_| "Progress UART output failed")?;

    Ok(())
}

fn run_test<O: ByteOutput>(
    logger: &mut Logger<O, LOG_CAPACITY>,
) -> Result<&'static str, &'static str> {
    if SECTOR_COUNT != 2 || SLOTS_PER_SECTOR != 8 {
        return Err("Rotation test requires two eight-slot sectors");
    }

    // SAFETY:
    // This isolated Pico test preserves the standard XIP configuration.
    // No core1, DMA, independent writer or FlashJournalReader is active.
    // No unmasked exception or debugger operation may access flash
    // during backend program/erase operations.
    let mut storage = unsafe { FlashJournalStorage::new() }
        .map_err(|_| "Invalid journal linker layout")?;

    let mut checksum = SoftwareCrc32;
    let existing = load::<SERVO_COUNT, _>(&mut storage, &mut checksum)
        .map_err(|_| "Initial journal load failed")?;

    if let Some(existing) = existing {
        let sequence = existing.record.sequence;

        if sequence == FINAL_SEQUENCE || sequence == COMPLETION_SEQUENCE {
            // Sequence alone is not permission to erase. Require the exact
            // synthetic fixture and its expected physical journal slot.
            if existing.record.calibrations != calibration_fixture(sequence)? {
                return Err("Unexpected valid record; journal unchanged");
            }
            let expected_slot = (sequence as usize - 1) % SLOT_COUNT;
            if existing.slot_index != expected_slot {
                return Err("Completion record is in an unexpected slot");
            }

            if sequence == FINAL_SEQUENCE {
                // Boot 2: verify persisted sequence 25 before appending marker
                // 26 in the next slot. No destructive preparation on this boot.
                let marker = calibration_fixture(COMPLETION_SEQUENCE)?;
                let marker_slot =
                    (COMPLETION_SEQUENCE as usize - 1) % SLOT_COUNT;

                // Keep this boot non-destructive: append must find slot 9
                // directly, without skipping dirty slots or reclaiming a sector.
                let mut slot = [0_u8; SLOT_SIZE];
                for index in marker_slot..SLOT_COUNT {
                    storage
                        .read_slot(index, &mut slot)
                        .map_err(
                            |_| "Completion marker preparation read failed",
                        )?;
                    if slot.iter().any(|&byte| byte != 0xFF) {
                        return Err("Completion marker slots are not erased");
                    }
                }
                log_progress(
                    logger,
                    "write",
                    COMPLETION_SEQUENCE,
                    marker_slot,
                )?;

                let written = append(&mut storage, &marker, &mut checksum)
                    .map_err(|_| "Completion marker append failed")?;
                if written.slot_index != marker_slot
                    || written.record.sequence != COMPLETION_SEQUENCE
                    || written.record.calibrations != marker
                {
                    return Err(
                        "Unexpected completion marker location or values",
                    );
                }

                let loaded =
                    load::<SERVO_COUNT, _>(&mut storage, &mut checksum)
                        .map_err(|_| "Completion marker reload failed")?
                        .ok_or("Completion marker not found")?;
                if loaded.slot_index != written.slot_index
                    || loaded.record != written.record
                {
                    return Err("Reloaded completion marker differs");
                }
                log_progress(
                    logger,
                    "verified",
                    COMPLETION_SEQUENCE,
                    written.slot_index,
                )?;
                return Ok(
                    "Journal rotation OK: completion sequence 26 written",
                );
            }

            // Boot 3: marker 26 survived restart and matches its fixture.
            // Fall through to the explicit preparation below, then run 1..25.
        } else if sequence != 1
            || existing.record.calibrations != calibration_fixture(1)?
        {
            // Do not automatically erase unexpected or intermediate records.
            return Err("Unexpected valid record; journal unchanged");
        }
    }

    logger
        .log(
            LogLevel::Info,
            "Journal preparation started",
        )
        .map_err(|_| "Progress log buffer full")?;
    logger
        .service(128)
        .map_err(|_| "Progress UART output failed")?;

    // Explicitly destructive preparation of this test journal.
    // Allowed for no valid record, the known initial fixture, or marker 26.
    for sector in 0..SECTOR_COUNT {
        storage
            .erase_sector(sector)
            .map_err(|_| "Journal preparation erase failed")?;
    }

    let mut actual = [0_u8; SLOT_SIZE];
    for index in 0..SLOT_COUNT {
        storage
            .read_slot(index, &mut actual)
            .map_err(|_| "Preparation read failed")?;

        if actual.iter().any(|&byte| byte != 0xFF) {
            return Err("Prepared journal is not completely erased");
        }
    }

    logger
        .log(
            LogLevel::Info,
            "Journal preparation complete",
        )
        .map_err(|_| "Progress log buffer full")?;
    logger
        .service(128)
        .map_err(|_| "Progress UART output failed")?;

    for sequence in 1..=FINAL_SEQUENCE {
        let expected = calibration_fixture(sequence)?;
        let expected_slot = (sequence as usize - 1) % SLOT_COUNT;

        let switching =
            sequence > 1 && (sequence as usize - 1) % SLOTS_PER_SECTOR == 0;

        let mut retained = [[0_u8; SLOT_SIZE]; SLOTS_PER_SECTOR];
        let retained_start = if switching {
            let previous_slot = (sequence as usize - 2) % SLOT_COUNT;
            let start = previous_slot / SLOTS_PER_SECTOR * SLOTS_PER_SECTOR;

            for (offset, slot) in retained.iter_mut().enumerate() {
                storage
                    .read_slot(start + offset, slot)
                    .map_err(|_| "Retained sector snapshot failed")?;
            }

            start
        } else {
            0
        };

        log_progress(logger, "write", sequence, expected_slot)?;

        let written = append(&mut storage, &expected, &mut checksum)
            .map_err(|_| "Rotation append failed")?;

        if written.slot_index != expected_slot
            || written.record.sequence != sequence
            || written.record.calibrations != expected
        {
            return Err("Append returned unexpected slot or record");
        }

        let loaded = load::<SERVO_COUNT, _>(&mut storage, &mut checksum)
            .map_err(|_| "Rotation reload failed")?
            .ok_or("Rotation record not found")?;

        if loaded.slot_index != written.slot_index
            || loaded.record != written.record
        {
            return Err("Reloaded rotation record differs");
        }

        if switching {
            for (offset, original) in retained.iter().enumerate() {
                storage
                    .read_slot(retained_start + offset, &mut actual)
                    .map_err(|_| "Retained sector verification failed")?;

                if actual != *original {
                    return Err("Sector switch changed previous sector");
                }
            }

            // Only the first slot of the new sector is occupied.
            // This also checks removal of old records at sequences 17/25.
            for offset in 1..SLOTS_PER_SECTOR {
                storage
                    .read_slot(expected_slot + offset, &mut actual)
                    .map_err(|_| "Target sector verification failed")?;

                if actual.iter().any(|&byte| byte != 0xFF) {
                    return Err("Switched sector contains stale slots");
                }
            }
        }
        log_progress(
            logger,
            "verified",
            sequence,
            written.slot_index,
        )?;
    }

    Ok("Journal rotation OK: sequence 25 written")
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

    let mut logger = Logger::<_, LOG_CAPACITY>::new(UartLogOutput::new(uart));
    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let (level, message) = match run_test(&mut logger) {
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
