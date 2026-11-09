//! Read-only access to the linker-reserved Pico calibration journal.

use core::ptr;

use hexaroam_servo_storage::{
    JOURNAL_SIZE, JournalLayout, JournalRead, LayoutError, SLOT_SIZE,
};

const FLASH_START: usize = 0x1000_0000;
const FLASH_SIZE: u32 = 2 * 1024 * 1024;

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
    layout: JournalLayout,
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

        if start < FLASH_START
            || end > FLASH_START + FLASH_SIZE as usize
            || end.checked_sub(start) != Some(JOURNAL_SIZE)
        {
            return Err(FlashReadError::InvalidLayout);
        }

        // The bounds above ensure this offset fits into u32.
        let start_offset = (start - FLASH_START) as u32;

        let layout = JournalLayout::new(start_offset, FLASH_SIZE)
            .map_err(|_| FlashReadError::InvalidLayout)?;

        Ok(Self { layout })
    }
}

impl JournalRead for FlashJournalReader {
    type Error = FlashReadError;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        let range = self
            .layout
            .slot_range(slot_index)
            .map_err(|error| match error {
                LayoutError::UnknownSlot(index) => {
                    FlashReadError::UnknownSlot(index)
                }
                _ => FlashReadError::InvalidLayout,
            })?;

        let address = FLASH_START + range.offset as usize;

        // SAFETY:
        // Construction establishes a readable, reserved XIP region
        // whose lifetime excludes flash modification/reconfiguration.
        // JournalLayout bounds the complete slot inside that region
        // and returns exactly SLOT_SIZE bytes.
        // The destination is a separate, writable RAM buffer.
        unsafe {
            ptr::copy_nonoverlapping(
                address as *const u8,
                output.as_mut_ptr(),
                range.length,
            );
        }

        Ok(())
    }
}
