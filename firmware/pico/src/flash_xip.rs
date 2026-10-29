//! RAM-resident XIP transitions and journal slot programming.

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

pub type RomProgramFunction =
    unsafe extern "C" fn(offset: u32, data: *const u8, count: usize);

/// Programs one complete journal slot while executing from RAM.
///
/// # Safety
///
/// This function and its stack must reside in RAM.
/// Interrupts must remain masked until XIP has been restored.
/// No other core, DMA channel or debugger operation may access flash.
///
/// connect/exit/program/flush must point to the corresponding
/// RP2040 ROM functions. restore must point to a valid, live RAM
/// copy of the current Boot2.
///
/// offset must identify a complete, erased SLOT_SIZE-byte slot
/// inside the linker-reserved calibration journal.
/// data must point to SLOT_SIZE readable bytes in RAM that remain
/// unchanged throughout this call.
///
/// The test assumes the standard Pico flash/pad configuration.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".data.ram_func")]
#[inline(never)]
pub unsafe extern "C" fn hexaroam_program_slot(
    connect: RomFunction,
    exit: RomFunction,
    program: RomProgramFunction,
    flush: RomFunction,
    restore: RomFunction,
    offset: u32,
    data: *const u8,
) {
    unsafe {
        connect();
        exit();
        program(
            offset,
            data,
            hexaroam_servo_storage::SLOT_SIZE,
        );
        flush();
        restore();
    }
}
