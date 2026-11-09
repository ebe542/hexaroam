#![no_std]
#![no_main]

//! Manual 24LC256 hardware write-protection test.
//!
//! Requires WP high and a test EEPROM without required calibration.
//! If protection is ineffective, overwrites 0x7F80..=0x7FFF.
//!
//! I2C1: GP18 SDA, GP19 SCL, 400 kHz, device address 0x50.
//! Supply 3.3 V, separate 2k pull-ups and local 100nF.
//!
//! UART1: GP20 -> adapter TTL RXD, common GND.
//! Terminal: 115200 baud, 8N1, no flow control, LF.
//!
//! Reads existing bytes, attempts to write their bitwise inverse,
//! then verifies that the complete region remained unchanged.
//! No automatic restoration, servo output or DMA.
//!
//! Run after a successful write/read test on the same EEPROM.
//! Unchanged contents alone do not prove that WP caused protection.

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

fn run_test<I: I2c>(
    eeprom: &mut Eeprom<I>,
    delay: &mut impl DelayNs,
) -> Result<(), &'static str> {
    let mut before = [0_u8; TEST_SIZE];

    eeprom
        .read(TEST_OFFSET, &mut before)
        .map_err(|_| "EEPROM WP initial read failed")?;

    // Every bit differs, regardless of the original contents.
    let attempted = before.map(|byte| !byte);

    // With WP high, the EEPROM acknowledges without storing data.
    // A bus error is not evidence of successful write protection.
    eeprom
        .write(TEST_OFFSET, &attempted, delay)
        .map_err(|_| "EEPROM WP write attempt failed")?;

    let mut after = [0_u8; TEST_SIZE];

    eeprom
        .read(TEST_OFFSET, &mut after)
        .map_err(|_| "EEPROM WP final read failed")?;

    if after != before {
        return Err("EEPROM WP failed: contents changed");
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
            "EEPROM WP test started: WP must be high",
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
            (
                LogLevel::Info,
                "EEPROM write protection OK",
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
            .expect("Debug UART output is infallible");
    }
}
