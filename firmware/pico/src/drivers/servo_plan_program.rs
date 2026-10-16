//! Multi-channel servo signal program.
//!
//! Assembly validation only; not yet installed or executed.

// Temporary until the DMA-backed driver uses this function.
#[allow(dead_code)]
pub fn program() -> pio::Program<32> {
    pio_proc::pio_asm!(
        ".wrap_target",
        "set x, 0",
        "pull noblock",
        "mov y, osr",
        "jmp !y fault",
        "mov isr, osr",
        "pull noblock",
        "mov y, osr",
        "jmp !y fault",
        "mov osr, isr",
        "out pins, 32",
        "delay:",
        "jmp y-- delay",
        ".wrap",
        "fault:",
        "mov osr, null",
        "out pins, 32",
        "irq 0",
        "halt:",
        "jmp halt",
    )
    .program
}
