#![no_std]
#![no_main]

//! Manual, destructive 24LC256 write/read hardware test.
//!
//! Overwrites 0x7F80..=0x7FFF on every boot.
//! Use a test EEPROM without required calibration data.
//!
//! I2C1: GP18 SDA, GP19 SCL, 400 kHz, device address 0x50.
//! Supply 3.3 V, WP low, separate 2k pull-ups and local 100nF.
//!
//! UART1: GP20 -> adapter TTL RXD, common GND.
//! Terminal: 115200 baud, 8N1, no flow control, LF.
//!
//! Writes a baseline, then patches across a physical page boundary.
//! Verifies the complete test region and the preceding guard region.
//! No servo output or DMA is started.
//! This is not a power-cycle persistence test.

mod support;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use embedded_hal::{delay::DelayNs, digital::OutputPin, i2c::I2c};
use hexaroam_eeprom::Eeprom;
use hexaroam_logging::{LogLevel, Logger};
use rp_pico::{
    entry,
    hal::{
        I2C,
        clocks::Clock,
        fugit::RateExtU32,
        gpio::{FunctionI2C, FunctionUart, Pin, PullNone},
        uart::{Pins, UartConfig, UartPeripheral},
    },
};
use uart_logging::UartLogOutput;

const DEVICE_ADDRESS: u8 = 0x50;

const TEST_OFFSET: u16 = 0x7F80;
const TEST_SIZE: usize = 128;

const GUARD_SIZE: usize = 64;
const BASE_PATTERN: u8 = 0xA5;

// Offset 61 leaves three bytes in the first 64-byte page.
// The remaining five patch bytes belong to the following page.
const PATCH_OFFSET: usize = 61;
const PATCH: [u8; 8] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];

fn run_test<I: I2c>(
    eeprom: &mut Eeprom<I>,
    delay: &mut impl DelayNs,
) -> Result<(), &'static str> {
    let guard_offset = TEST_OFFSET - GUARD_SIZE as u16;
    let mut guard_before = [0_u8; GUARD_SIZE];

    eeprom
        .read(guard_offset, &mut guard_before)
        .map_err(|_| "EEPROM initial guard read failed")?;

    // Deliberately replace the full test region on every run.
    let mut expected = [BASE_PATTERN; TEST_SIZE];

    eeprom
        .write(TEST_OFFSET, &expected, delay)
        .map_err(|_| "EEPROM baseline write failed")?;

    let mut actual = [0_u8; TEST_SIZE];

    eeprom
        .read(TEST_OFFSET, &mut actual)
        .map_err(|_| "EEPROM baseline read failed")?;

    if actual != expected {
        return Err("EEPROM baseline mismatch; check WP");
    }

    // The byte driver must split this into writes of 3 and 5 bytes.
    eeprom
        .write(
            TEST_OFFSET + PATCH_OFFSET as u16,
            &PATCH,
            delay,
        )
        .map_err(|_| "EEPROM cross-page write failed")?;

    expected[PATCH_OFFSET..PATCH_OFFSET + PATCH.len()].copy_from_slice(&PATCH);

    eeprom
        .read(TEST_OFFSET, &mut actual)
        .map_err(|_| "EEPROM final read failed")?;

    // Includes both the patch and all surrounding baseline bytes.
    if actual != expected {
        return Err("EEPROM cross-page readback mismatch");
    }

    let mut guard_after = [0_u8; GUARD_SIZE];

    eeprom
        .read(guard_offset, &mut guard_after)
        .map_err(|_| "EEPROM final guard read failed")?;

    if guard_after != guard_before {
        return Err("EEPROM bytes before test region changed");
    }

    Ok(())
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

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    // External 2k pull-ups define the bus.
    // Disable the Pico's internal pull resistors explicitly.
    let sda: Pin<_, FunctionI2C, PullNone> = board.pins.gpio18.reconfigure();

    let scl: Pin<_, FunctionI2C, PullNone> = board.pins.gpio19.reconfigure();

    let i2c = I2C::new_controller(
        board.i2c1,
        sda,
        scl,
        400_u32.kHz(),
        &mut board.resets,
        board.system_clock_hz.Hz(),
    );

    let mut eeprom = Eeprom::new(i2c, DEVICE_ADDRESS)
        .expect("Invalid EEPROM device address");

    logger
        .log(
            LogLevel::Info,
            "EEPROM write/read started: 0x7F80..0x7FFF",
        )
        .expect("Start message must fit");

    // Send the start message before entering blocking I2C access.
    while logger.queued_bytes() > 0 {
        logger
            .service(16)
            .expect("Debug UART output is infallible");
    }

    let (level, message) = match run_test(&mut eeprom, &mut board.timer) {
        Ok(()) => {
            led.set_high()
                .expect("Failed to turn LED on");
            (LogLevel::Info, "EEPROM write/read OK")
        }
        Err(message) => (LogLevel::Error, message),
    };

    logger
        .log(level, message)
        .expect("Result message must fit");

    loop {
        logger
            .service(16)
            .expect("Debug UART output is infallible");
    }
}
