#![no_std]
#![no_main]

//! Manual UART calibration persistence test without servo output.
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
//! - ProtocolVersion: 2
//! - Status: initially 0; Busy and save-result fields change during requests
//! - ServoCount: 0
//!
//! Valid handled requests produce ACK/NACK replies.
//! Synthetic calibration requests update the reserved flash journal.
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

#[path = "../src/flash_journal_storage.rs"]
mod flash_journal_storage;

#[path = "../src/flash_xip.rs"]
mod flash_xip;

#[path = "support/observed_journal_storage.rs"]
mod observed_journal_storage;

use core::fmt::{self, Write};
use embedded_hal::digital::OutputPin;
use flash_journal_storage::FlashJournalStorage;
use hexaroam_logging::{ByteOutput, LogLevel, Logger};
use hexaroam_servo_control::{Angle, Calibration};
use hexaroam_servo_registers::{
    FrameProcessingError, FrameProcessor, RegisterService, persist_calibration,
};
use hexaroam_servo_storage::{Checksum, LocatedRecord, SoftwareCrc32, load};
use no_servo_output::NoServoOutput;
use observed_journal_storage::ObservedJournalStorage;
use rp_pico::{
    entry,
    hal::{
        clocks::Clock,
        gpio::FunctionUart,
        uart::{Pins, UartConfig, UartPeripheral},
    },
};
use uart_logging::UartLogOutput;
use uart_transport::{UartTransport, UartTransportError};

// Initial manual-test value, not a production timing guarantee.
const RECEIVE_IDLE_TIMEOUT_US: u32 = 100_000;
const RECEIVE_BUDGET: usize = 32;
const TRANSMIT_BUDGET: usize = 32;
const SERVO_COUNT: usize = 18;
const LOG_CAPACITY: usize = 512;

type TestProcessor = FrameProcessor<NoServoOutput>;

fn checksum(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

struct TestMessage {
    bytes: [u8; 128],
    len: usize,
}

impl Write for TestMessage {
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

fn log_message<O: ByteOutput>(
    logger: &mut Logger<O, LOG_CAPACITY>,
    arguments: fmt::Arguments<'_>,
) -> Result<(), &'static str> {
    let mut message = TestMessage {
        bytes: [0; 128],
        len: 0,
    };

    message
        .write_fmt(arguments)
        .map_err(|_| "Test message formatting failed")?;

    let text = core::str::from_utf8(&message.bytes[..message.len])
        .map_err(|_| "Invalid test message encoding")?;

    logger
        .log(LogLevel::Info, text)
        .map_err(|_| "Test log buffer full")
}

fn fixture_name(
    calibrations: &[Option<Calibration>; SERVO_COUNT],
) -> &'static str {
    for variant in 0..2_u32 {
        let matches = calibrations
            .iter()
            .enumerate()
            .all(|(id, entry)| {
                let angle_offset = id as i32 * 10;
                let pulse_offset = id as u32 * 5;

                let expected = Calibration::new(
                    Angle::from_centidegrees(-4_500 + angle_offset),
                    1_000 + pulse_offset + variant * 100,
                    Angle::from_centidegrees(4_500 + angle_offset),
                    2_000 - pulse_offset + variant * 100,
                )
                .expect("Synthetic fixture must be valid");

                *entry == Some(expected)
            });

        if matches {
            return if variant == 0 { "A" } else { "B" };
        }
    }

    "other"
}

fn log_boot_record<O: ByteOutput>(
    logger: &mut Logger<O, LOG_CAPACITY>,
    record: Option<LocatedRecord<SERVO_COUNT>>,
) -> Result<(), &'static str> {
    match record {
        Some(record) => log_message(
            logger,
            format_args!(
                "Boot journal: seq={} slot={} fixture={}",
                record.record.sequence,
                record.slot_index,
                fixture_name(&record.record.calibrations),
            ),
        ),
        None => log_message(
            logger,
            format_args!("Boot journal: no valid record"),
        ),
    }
}

fn persist_and_check<O: ByteOutput>(
    processor: &mut TestProcessor,
    storage: &mut ObservedJournalStorage,
    logger: &mut Logger<O, LOG_CAPACITY>,
) -> Result<(), &'static str> {
    let Some(pending) = processor.calibration_ready() else {
        return Ok(());
    };

    // Copy the expected values before completion releases the request.
    let expected = *pending.calibrations();

    let before = load::<SERVO_COUNT, _>(storage, &mut SoftwareCrc32)
        .map_err(|_| "Journal load before persistence failed")?;

    let writes_before = storage.write_calls();
    let erases_before = storage.erase_calls();

    let completed = persist_calibration(processor, storage, &mut SoftwareCrc32)
        .map_err(|_| "Calibration persistence failed")?;

    if !completed {
        return Err("Ready request was not completed");
    }

    if processor.calibration_ready().is_some() {
        return Err("Completed request was not released");
    }

    let after = load::<SERVO_COUNT, _>(storage, &mut SoftwareCrc32)
        .map_err(|_| "Journal reload failed")?
        .ok_or("Persisted record not found")?;

    if after.record.calibrations != expected {
        return Err("Persisted calibration differs from UART request");
    }

    let unchanged =
        before.is_some_and(|record| record.record.calibrations == expected);

    if unchanged {
        let previous = before.ok_or("Missing comparison record")?;

        if after != previous
            || storage.write_calls() != writes_before
            || storage.erase_calls() != erases_before
        {
            return Err("Identical calibration modified the journal");
        }
    } else {
        let expected_sequence = match before {
            Some(previous) => previous
                .record
                .sequence
                .checked_add(1)
                .ok_or("Journal sequence overflow")?,
            None => 1,
        };

        if after.record.sequence != expected_sequence
            || storage.write_calls() != writes_before + 1
            || storage.erase_calls() - erases_before > 1
        {
            return Err("Changed calibration has unexpected journal progress");
        }
    }

    log_message(
        logger,
        format_args!(
            "Persist OK: seq={} fixture={} unchanged={} writes={} erases={}",
            after.record.sequence,
            fixture_name(&after.record.calibrations),
            unchanged,
            storage.write_calls(),
            storage.erase_calls(),
        ),
    )
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

    let mut logger =
        Logger::<_, LOG_CAPACITY>::new(UartLogOutput::new(debug_uart));

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

    // SAFETY:
    // This standalone test preserves standard Pico XIP/flash configuration.
    // It starts neither core1 nor DMA and owns the only flash journal backend.
    // No independent writer or FlashJournalReader coexists with this object.
    // No debugger operation or unmasked exception may access flash during
    // program/erase operations; the backend masks normal interrupts.
    let flash = unsafe { FlashJournalStorage::new() }
        .expect("Invalid journal linker layout");

    let mut storage = ObservedJournalStorage::new(flash);

    let existing = load::<SERVO_COUNT, _>(&mut storage, &mut SoftwareCrc32)
        .expect("Initial journal load failed");

    log_boot_record(&mut logger, existing)
        .expect("Boot record message must fit");

    logger
        .log(
            LogLevel::Info,
            "UART calibration persistence ready",
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
            Err(FrameProcessingError::Receive(error)) => {
                processor.on_receive_timeout();
                receive_timeout_armed = false;

                let message = match error {
                    UartTransportError::Overrun => "Protocol UART RX overrun",
                    UartTransportError::Break => "Protocol UART RX break",
                    UartTransportError::Parity => {
                        "Protocol UART RX parity error"
                    }
                    UartTransportError::Framing => {
                        "Protocol UART RX framing error"
                    }
                };

                let _ = logger.log(LogLevel::Error, message);
            }
            Err(error) => {
                panic!(
                    "Protocol processing failed: {:?}",
                    error
                );
            }
        }

        if let Err(message) = persist_and_check(
            &mut processor,
            &mut storage,
            &mut logger,
        ) {
            led.set_high()
                .expect("Failed to indicate test failure");

            panic!(
                "Calibration hardware test failed: {}",
                message
            );
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
