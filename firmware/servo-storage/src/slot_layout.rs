//! Checked slot ranges without erase-sector assumptions.

use crate::{SLOT_SIZE, StorageRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotLayoutError {
    EmptyRegion,
    MisalignedStart,
    RegionOutOfBounds,
    UnknownSlot(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotLayout {
    start: u32,
    slot_count: usize,
}

impl SlotLayout {
    /// Creates a region of consecutive fixed-size journal slots.
    ///
    /// The region must contain at least one slot, start on a
    /// slot boundary and fit entirely within storage_size.
    /// Arithmetic overflow is reported as RegionOutOfBounds.
    pub fn new(
        start: u32,
        slot_count: usize,
        storage_size: u32,
    ) -> Result<Self, SlotLayoutError> {
        if slot_count == 0 {
            return Err(SlotLayoutError::EmptyRegion);
        }

        if start % SLOT_SIZE as u32 != 0 {
            return Err(SlotLayoutError::MisalignedStart);
        }

        let length = slot_count
            .checked_mul(SLOT_SIZE)
            .and_then(|length| u32::try_from(length).ok())
            .ok_or(SlotLayoutError::RegionOutOfBounds)?;

        let end = start
            .checked_add(length)
            .ok_or(SlotLayoutError::RegionOutOfBounds)?;

        if end > storage_size {
            return Err(SlotLayoutError::RegionOutOfBounds);
        }

        Ok(Self { start, slot_count })
    }

    pub fn slot_count(&self) -> usize {
        self.slot_count
    }

    /// Returns the byte range of one slot inside the region.
    pub fn slot_range(
        &self,
        slot_index: usize,
    ) -> Result<StorageRange, SlotLayoutError> {
        if slot_index >= self.slot_count {
            return Err(SlotLayoutError::UnknownSlot(slot_index));
        }

        // Construction validated the complete region, so every
        // valid slot index fits within both u32 and the storage.
        Ok(StorageRange {
            offset: self.start + (slot_index * SLOT_SIZE) as u32,
            length: SLOT_SIZE,
        })
    }
}

#[cfg(test)]
#[path = "test_slot_layout.rs"]
mod tests;
