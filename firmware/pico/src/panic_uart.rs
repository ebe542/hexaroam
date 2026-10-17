//! Best-effort panic output through UART1.
//!
//! Requires exclusive UART1 access during panic handling:
//! core 1 must not use UART1, and no DMA may write to it.
//! The handler disables interrupts and never returns.

use core::{convert::Infallible, fmt::Write, panic::PanicInfo};

use hexaroam_logging::{BoundedWriter, ByteOutput};
use rp_pico::hal::pac;

// Total write attempts, including retries while the FIFO is full.
// This is a finite attempt budget, not a calibrated time limit.
const MAX_WRITE_ATTEMPTS: usize = 1_000_000;

struct PanicUartOutput {
    registers: &'static pac::uart0::RegisterBlock,
}

impl ByteOutput for PanicUartOutput {
    type Error = Infallible;

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        if self
            .registers
            .uartfr()
            .read()
            .txff()
            .bit_is_set()
        {
            return Ok(false);
        }

        self.registers.uartdr().write(|w| {
            // SAFETY: every u8 is a valid UART data value.
            unsafe { w.data().bits(byte) }
        });

        Ok(true)
    }
}

fn report_panic(info: &PanicInfo<'_>) {
    // SAFETY: interrupts are disabled and this handler never returns.
    // Core 1 and DMA do not access UART1 in this firmware.
    // Only shared register references are created.
    let resets = unsafe { &*pac::RESETS::ptr() };

    // Avoid accessing UART registers while the peripheral is in reset.
    if resets
        .reset_done()
        .read()
        .uart1()
        .bit_is_clear()
    {
        return;
    }

    // SAFETY: the same exclusive-access conditions apply here.
    let registers = unsafe { &*pac::UART1::ptr() };

    let control = registers.uartcr().read();

    if control.uarten().bit_is_clear() || control.txe().bit_is_clear() {
        return;
    }

    let mut output = PanicUartOutput { registers };
    let mut writer = BoundedWriter::new(&mut output, MAX_WRITE_ATTEMPTS);

    // Start a new line in case normal output was interrupted.
    // PanicInfo includes the message and source location.
    let _ = writeln!(writer, "\nPANIC {info}");
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    cortex_m::interrupt::disable();

    report_panic(info);

    // UART hardware continues transmitting accepted FIFO bytes.
    // Keep peripheral clocks running while stopping CPU execution.
    loop {
        cortex_m::asm::nop();
    }
}
