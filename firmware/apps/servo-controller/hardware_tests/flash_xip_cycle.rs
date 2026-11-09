#![no_std]
#![no_main]

//! Manual hardware test: leave and restore XIP without modifying flash.
//!
//! UART1: GP20 -> adapter TTL RXD, shared GND, 3.3-V logic.
//! Terminal: 115200 baud, 8N1, no flow control.
//!
//! Expected: INFO XIP cycle OK, LED permanently high.
//! No flash program/erase, second core, DMA or servo output is started.

mod support;

#[path = "../src/flash_xip.rs"]
mod flash_xip;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

use core::{
    mem, ptr,
    sync::atomic::{Ordering, compiler_fence},
};

use embedded_hal::digital::OutputPin;
use flash_xip::{RomFunction, hexaroam_xip_cycle};
use hexaroam_logging::{LogLevel, Logger};
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::{
    clocks::Clock,
    gpio::FunctionUart,
    rom_data,
    uart::{Pins, UartConfig, UartPeripheral},
};
use uart_logging::UartLogOutput;

const FLASH_START: usize = 0x1000_0000;
const RAM_START: usize = 0x2000_0000;
const RAM_END: usize = 0x2004_0000;
const BOOT2_WORDS: usize = 64;

fn rom_address_valid(function: RomFunction) -> bool {
    let address = function as *const () as usize;

    // RP2040 ROM occupies the first 16 KiB.
    // Callable Cortex-M functions require the Thumb-state bit.
    address & 1 != 0 && (address & !1_usize) < 0x4000
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

    // Resolve HAL ROM lookups while ordinary flash access still works.
    let connect = rom_data::connect_internal_flash::ptr();
    let exit = rom_data::flash_exit_xip::ptr();
    let flush = rom_data::flash_flush_cache::ptr();

    // u32 storage gives the Boot2 copy four-byte alignment.
    let mut boot2 = [0_u32; BOOT2_WORDS];

    for (index, word) in boot2.iter_mut().enumerate() {
        // SAFETY:
        // The Pico is running normally from its mapped flash.
        // These reads stay inside the current 256-byte Boot2 image.
        *word = unsafe {
            ptr::read_volatile((FLASH_START as *const u32).add(index))
        };
    }

    let boot2_address = boot2.as_ptr() as usize;
    let routine_address = (hexaroam_xip_cycle as *const () as usize) & !1_usize;

    let addresses_valid = (RAM_START..RAM_END).contains(&routine_address)
        && boot2_address >= RAM_START
        && boot2_address + size_of_val(&boot2) <= RAM_END
        && rom_address_valid(connect)
        && rom_address_valid(exit)
        && rom_address_valid(flush);

    let (level, message, success) = if !addresses_valid {
        (
            LogLevel::Error,
            "XIP preparation address check failed",
            false,
        )
    } else {
        // SAFETY:
        // Boot2 was copied from this running image into aligned RAM.
        // Set the Thumb-state bit; the copy remains live during the call.
        let restore =
            unsafe { mem::transmute::<usize, RomFunction>(boot2_address | 1) };

        compiler_fence(Ordering::SeqCst);

        // SAFETY:
        // This standalone test never starts core1 or DMA.
        // No reader instance, flash writer or servo output is active.
        // The RAM entry and ROM pointers were checked above.
        // Interrupts remain masked until the RAM routine restores XIP.
        cortex_m::interrupt::free(|_| unsafe {
            hexaroam_xip_cycle(connect, exit, flush, restore);
        });

        compiler_fence(Ordering::SeqCst);

        // Read flash again only after the restoration routine returned.
        let flash_word =
            unsafe { ptr::read_volatile(FLASH_START as *const u32) };

        if flash_word == boot2[0] {
            (LogLevel::Info, "XIP cycle OK", true)
        } else {
            (
                LogLevel::Error,
                "Flash read mismatch after XIP restoration",
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
