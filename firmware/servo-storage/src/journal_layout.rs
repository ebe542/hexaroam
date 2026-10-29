//! Checked byte ranges for a journal inside a storage device.

use crate::{JOURNAL_SIZE, SECTOR_COUNT, SECTOR_SIZE, SLOT_COUNT, SLOT_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutError {
    MisalignedStart,
    RegionOutOfBounds,
    UnknownSlot(usize),
    UnknownSector(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageRange {
    pub offset: u32,
    pub length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalLayout {
    start: u32,
}

impl JournalLayout {
    pub fn new(start: u32, storage_size: u32) -> Result<Self, LayoutError> {
        if start % SECTOR_SIZE as u32 != 0 {
            return Err(LayoutError::MisalignedStart);
        }

        let end = start
            .checked_add(JOURNAL_SIZE as u32)
            .ok_or(LayoutError::RegionOutOfBounds)?;

        if end > storage_size {
            return Err(LayoutError::RegionOutOfBounds);
        }

        Ok(Self { start })
    }

    pub fn slot_range(
        &self,
        slot_index: usize,
    ) -> Result<StorageRange, LayoutError> {
        if slot_index >= SLOT_COUNT {
            return Err(LayoutError::UnknownSlot(slot_index));
        }

        Ok(StorageRange {
            offset: self.start + (slot_index * SLOT_SIZE) as u32,
            length: SLOT_SIZE,
        })
    }

    pub fn sector_range(
        &self,
        sector_index: usize,
    ) -> Result<StorageRange, LayoutError> {
        if sector_index >= SECTOR_COUNT {
            return Err(LayoutError::UnknownSector(sector_index));
        }

        Ok(StorageRange {
            offset: self.start + (sector_index * SECTOR_SIZE) as u32,
            length: SECTOR_SIZE,
        })
    }
}

#[cfg(test)]
#[path = "test_journal_layout.rs"]
mod tests;
