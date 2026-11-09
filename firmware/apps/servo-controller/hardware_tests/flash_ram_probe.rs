#![no_std]
#![no_main]

//! Manual hardware test: RAM-resident code placement and execution.
//!
//! UART1: GP20 -> adapter TTL RXD, shared GND, 3.3-V logic.
//! Terminal: 115200 baud, 8N1, no flow control.
//!
//! Expected: INFO RAM routine OK, LED permanently high.
//! XIP remains enabled. No flash programming/erase or servo output.
//! This does not yet verify a complete flash-operation call path.

mod support;

#[path = "../src/flash_ram.rs"]
mod flash_ram;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use embedded_hal::digital::OutputPin;
use flash_ram::hexaroam_ram_probe;
use hexaroam_logging::{LogLevel, Logger};
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

    let mut logger = Logger::<_, 128>::new(UartLogOutput::new(uart));
    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    // Cortex-M function pointers can contain the Thumb-state bit.
    let address = (hexaroam_ram_probe as *const () as usize) & !1_usize;

    let (level, message, success) = if !(0x2000_0000..0x2004_0000)
        .contains(&address)
    {
        (
            LogLevel::Error,
            "RAM routine address outside configured RAM",
            false,
        )
    } else {
        // Runtime input prevents a constant-input calculation.
        let input = board.timer.get_counter_low();

        // Only the probe itself must reside in RAM in this test.
        // XIP remains enabled, including for this wrapper.
        let result = cortex_m::interrupt::free(|_| hexaroam_ram_probe(input));

        if result == input ^ 0xA5A5_5A5A {
            (LogLevel::Info, "RAM routine OK", true)
        } else {
            (
                LogLevel::Error,
                "RAM routine returned an unexpected result",
                false,
            )
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
