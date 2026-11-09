#![no_std]
#![no_main]

//! Manual hardware test: erase journal sector 0 and verify it.
//!
//! Destroys all contents of sector 0, including the slot test pattern.
//! Checks all erased bytes and verifies that sector 1 is unchanged.
//! Refuses another erase if sector 0 is already completely erased.
//! Does not start core1, DMA or servo output.
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
use flash_xip::{RomFunction, hexaroam_erase_sector};
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_storage::{JOURNAL_SIZE, JournalLayout, SECTOR_SIZE};
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

    let target_range = layout
        .sector_range(0)
        .map_err(|_| "Invalid target sector")?;

    let retained_range = layout
        .sector_range(1)
        .map_err(|_| "Invalid retained sector")?;

    let target = (FLASH_START + target_range.offset as usize) as *const u8;
    let retained = (FLASH_START + retained_range.offset as usize) as *const u8;

    let mut occupied = false;
    for index in 0..SECTOR_SIZE {
        // SAFETY: XIP is active; JournalLayout bounds the whole sector.
        if unsafe { ptr::read_volatile(target.add(index)) } != 0xFF {
            occupied = true;
            break;
        }
    }

    if !occupied {
        return Err("Journal sector 0 is already erased");
    }

    // Keep an exact snapshot of the sector that must remain unchanged.
    let mut retained_before = [0_u8; SECTOR_SIZE];
    for (index, byte) in retained_before.iter_mut().enumerate() {
        // SAFETY: XIP is active; the complete retained sector is bounded.
        *byte = unsafe { ptr::read_volatile(retained.add(index)) };
    }

    // Resolve all ROM lookups before disabling XIP.
    let connect = rom_data::connect_internal_flash::ptr();
    let exit = rom_data::flash_exit_xip::ptr();
    let erase = rom_data::flash_range_erase::ptr();
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
        (hexaroam_erase_sector as *const () as usize) & !1_usize;

    let rom_addresses = [
        connect as *const () as usize,
        exit as *const () as usize,
        erase as *const () as usize,
        flush as *const () as usize,
    ];

    if !ram_range_valid(routine_address, 1)
        || !ram_range_valid(boot2_address, size_of_val(&boot2))
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
        // JournalLayout bounds the sector inside the reserved region.
        // This test deliberately discards all contents of sector 0.
        // Boot2 resides in live RAM; ROM pointers were checked above.
        // Interrupts stay masked until the RAM routine restores XIP.
        // The Pico uses its standard flash/pad configuration.
        unsafe {
            hexaroam_erase_sector(
                connect,
                exit,
                erase,
                flush,
                restore,
                target_range.offset,
            );
        }
    });

    compiler_fence(Ordering::SeqCst);

    let mut erased = true;
    let mut retained_unchanged = true;

    for index in 0..SECTOR_SIZE {
        // SAFETY: XIP is restored; both sectors were bounded above.
        if unsafe { ptr::read_volatile(target.add(index)) } != 0xFF {
            erased = false;
        }

        // SAFETY: Same readable, bounded retained sector.
        if unsafe { ptr::read_volatile(retained.add(index)) }
            != retained_before[index]
        {
            retained_unchanged = false;
        }
    }

    if !retained_unchanged {
        return Err("Journal sector 1 changed unexpectedly");
    }

    if !erased {
        return Err("Journal sector erase verification failed");
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
                "Journal sector erase OK",
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
