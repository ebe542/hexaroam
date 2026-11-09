#![no_std]
#![no_main]

//! Manual hardware test: program erased journal slot 0 and verify it.
//!
//! Writes a test pattern, not a valid calibration record.
//! Refuses to write if any byte of the target slot is not 0xFF.
//! Does not erase flash or start core1, DMA or servo output.
//!
//! UART1: GP20 -> adapter TTL RXD, shared GND, 3.3-V logic.
//! Terminal: 115200 baud, 8N1, no flow control.

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
use flash_xip::{RomFunction, hexaroam_program_slot};
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_storage::{JOURNAL_SIZE, JournalLayout, SLOT_SIZE};
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
const FLASH_SIZE: u32 = 2 * 1024 * 1024;
const RAM_START: usize = 0x2000_0000;
const RAM_END: usize = 0x2004_0000;
const BOOT2_WORDS: usize = 64;

unsafe extern "C" {
    static __calibration_start: u8;
    static __calibration_end: u8;
}

fn ram_range_valid(address: usize, length: usize) -> bool {
    address >= RAM_START
        && address
            .checked_add(length)
            .is_some_and(|end| end <= RAM_END)
}

fn rom_address_valid(address: usize) -> bool {
    address & 1 != 0 && (address & !1_usize) < 0x4000
}

/// Runs only in this standalone test: core1 and DMA are never started.
fn run_test() -> Result<(), &'static str> {
    let start = ptr::addr_of!(__calibration_start) as usize;
    let end = ptr::addr_of!(__calibration_end) as usize;

    if start < FLASH_START
        || end > FLASH_START + FLASH_SIZE as usize
        || end.checked_sub(start) != Some(JOURNAL_SIZE)
    {
        return Err("Invalid journal linker region");
    }

    let layout = JournalLayout::new((start - FLASH_START) as u32, FLASH_SIZE)
        .map_err(|_| "Invalid journal layout")?;

    let range = layout
        .slot_range(0)
        .map_err(|_| "Invalid journal slot")?;

    let target = (FLASH_START + range.offset as usize) as *const u8;

    for index in 0..SLOT_SIZE {
        // SAFETY: XIP is active and the complete slot was bounded above.
        if unsafe { ptr::read_volatile(target.add(index)) } != 0xFF {
            return Err("Journal slot 0 is not erased");
        }
    }

    // Deliberately not a valid journal header.
    // Different byte values exercise both pages of the 512-byte slot.
    let mut data = [0_u8; SLOT_SIZE];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = (index as u8) ^ 0xA5;
    }

    // Resolve all ROM lookups before disabling XIP.
    let connect = rom_data::connect_internal_flash::ptr();
    let exit = rom_data::flash_exit_xip::ptr();
    let program = rom_data::flash_range_program::ptr();
    let flush = rom_data::flash_flush_cache::ptr();

    let mut boot2 = [0_u32; BOOT2_WORDS];
    for (index, word) in boot2.iter_mut().enumerate() {
        // SAFETY: Read the running image's Boot2 while XIP is active.
        *word = unsafe {
            ptr::read_volatile((FLASH_START as *const u32).add(index))
        };
    }

    let boot2_address = boot2.as_ptr() as usize;
    let routine_address =
        (hexaroam_program_slot as *const () as usize) & !1_usize;

    let rom_addresses = [
        connect as *const () as usize,
        exit as *const () as usize,
        program as *const () as usize,
        flush as *const () as usize,
    ];

    if !ram_range_valid(routine_address, 1)
        || !ram_range_valid(boot2_address, size_of_val(&boot2))
        || !ram_range_valid(data.as_ptr() as usize, data.len())
        || !rom_addresses
            .into_iter()
            .all(rom_address_valid)
    {
        return Err("Flash preparation address check failed");
    }

    // SAFETY: The aligned Boot2 copy remains live in RAM during the call.
    // Set the Cortex-M Thumb-state bit.
    let restore =
        unsafe { mem::transmute::<usize, RomFunction>(boot2_address | 1) };

    compiler_fence(Ordering::SeqCst);

    cortex_m::interrupt::free(|_| {
        // SAFETY:
        // This standalone test never starts core1 or DMA.
        // No FlashJournalReader instance exists.
        // The reserved target slot was checked to be completely erased.
        // Source data and Boot2 reside in live RAM.
        // All ROM pointers were resolved and checked with XIP active.
        // Interrupts stay masked until the RAM routine restores XIP.
        // The Pico uses its standard flash/pad configuration.
        unsafe {
            hexaroam_program_slot(
                connect,
                exit,
                program,
                flush,
                restore,
                range.offset,
                data.as_ptr(),
            );
        }
    });

    compiler_fence(Ordering::SeqCst);

    // Verify every byte only after XIP has been restored.
    for (index, expected) in data.iter().enumerate() {
        // SAFETY: XIP is restored; target remains within the reserved slot.
        let actual = unsafe { ptr::read_volatile(target.add(index)) };

        if actual != *expected {
            return Err("Journal slot readback mismatch");
        }
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
                "Journal slot program OK",
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
