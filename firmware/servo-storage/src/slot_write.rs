//! Byte-range writes within journal slots.

use crate::{JournalRead, SlotError};

pub trait SlotWrite: JournalRead {
    /// Overwrites bytes within one slot.
    ///
    /// The backend must validate the slot and byte range before
    /// modification. Success must not change bytes outside the range.
    /// Success also requires completion of the device write cycle.
    ///
    /// An error may leave the requested range partially modified.
    fn write_slot_bytes(
        &mut self,
        slot_index: usize,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotWriteError<E> {
    InvalidServoCount,
    Slot(SlotError),
    Storage(E),
    InvalidationFailed,
    VerificationFailed,
}
