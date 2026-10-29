//! Read-only access to the linker-reserved Pico calibration journal.

use core::ptr;

use hexaroam_servo_storage::{JournalRead, SLOT_COUNT, SLOT_SIZE};

unsafe extern "C" {
    static __calibration_start: u8;
    static __calibration_end: u8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashReadError {
    InvalidLayout,
    UnknownSlot(usize),
}

pub struct FlashJournalReader {
    start: usize,
}

impl FlashJournalReader {
    /// Creates a reader for the reserved journal.
    ///
    /// # Safety
    ///
    /// The program must run on a Pico with readable 2-MiB XIP flash.
    /// For the reader's entire lifetime, no core, interrupt handler or
    /// other code may program/erase flash or disable/reconfigure XIP.
    /// The linker symbols must delimit the reserved journal region.
    pub unsafe fn new() -> Result<Self, FlashReadError> {
        let start = ptr::addr_of!(__calibration_start) as usize;
        let end = ptr::addr_of!(__calibration_end) as usize;

        let expected_size = SLOT_COUNT * SLOT_SIZE;

        if start < 0x1000_0000
            || end > 0x1020_0000
            || end.checked_sub(start) != Some(expected_size)
            || start % 4096 != 0
        {
            return Err(FlashReadError::InvalidLayout);
        }

        Ok(Self { start })
    }
}

impl JournalRead for FlashJournalReader {
    type Error = FlashReadError;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        if slot_index >= SLOT_COUNT {
            return Err(FlashReadError::UnknownSlot(slot_index));
        }

        let address = self.start + slot_index * SLOT_SIZE;

        // SAFETY:
        // Construction establishes a readable XIP region whose lifetime
        // excludes flash modification and XIP reconfiguration.
        // The checked index keeps the complete slot inside that region.
        // The destination is a separate, writable RAM buffer.
        unsafe {
            ptr::copy_nonoverlapping(
                address as *const u8,
                output.as_mut_ptr(),
                SLOT_SIZE,
            );
        }

        Ok(())
    }
}
