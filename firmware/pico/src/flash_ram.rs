//! RAM-resident routine used to verify code placement and execution.

/// Performs a simple operation without calling other functions.
///
/// The linker places .data.* in RAM, and startup copies it from flash.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".data.ram_func")]
#[inline(never)]
pub extern "C" fn hexaroam_ram_probe(value: u32) -> u32 {
    value ^ 0xA5A5_5A5A
}
