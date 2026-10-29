//! RAM-resident XIP transition without flash programming or erase.

pub type RomFunction = unsafe extern "C" fn();

/// Leaves XIP and restores it through a RAM copy of Boot2.
///
/// # Safety
///
/// This function must execute from RAM with interrupts masked.
/// No other core, DMA channel or debugger operation may access flash.
/// All supplied function pointers must be valid:
/// connect/exit/flush must point into RP2040 ROM;
/// restore must point to a valid, live RAM copy of the current Boot2.
/// The test assumes the standard Pico flash/pad configuration.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".data.ram_func")]
#[inline(never)]
pub unsafe extern "C" fn hexaroam_xip_cycle(
    connect: RomFunction,
    exit: RomFunction,
    flush: RomFunction,
    restore: RomFunction,
) {
    unsafe {
        connect();
        exit();
        flush();
        restore();
    }
}
