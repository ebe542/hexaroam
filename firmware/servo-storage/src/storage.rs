//! Storage interface for the two-sector calibration journal.

use crate::SLOT_SIZE;

pub const SECTOR_COUNT: usize = 2;
pub const SLOTS_PER_SECTOR: usize = 8;
pub const SLOT_COUNT: usize = SECTOR_COUNT * SLOTS_PER_SECTOR;
pub const SECTOR_SIZE: usize = SLOTS_PER_SECTOR * SLOT_SIZE;
pub const JOURNAL_SIZE: usize = SECTOR_COUNT * SECTOR_SIZE;

/// Storage operations used by the journal.
///
/// Read access to calibration journal slots.
pub trait JournalRead {
    type Error;

    /// Reads one complete slot.
    ///
    /// On error, the output buffer must not be interpreted.
    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error>;
}

/// Write access in addition to journal reading.
pub trait JournalStorage: JournalRead {
    /// Programs one completely erased slot.
    ///
    /// An error may leave the slot partially programmed.
    /// Success must not modify any other slot.
    fn write_slot(
        &mut self,
        slot_index: usize,
        data: &[u8; SLOT_SIZE],
    ) -> Result<(), Self::Error>;

    /// Erases one sector, setting all its bytes to 0xFF.
    ///
    /// An error may leave the sector partially erased.
    /// Success must not modify the other sector.
    fn erase_sector(&mut self, sector_index: usize) -> Result<(), Self::Error>;
}
