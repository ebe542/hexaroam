//! Finds the newest valid record in journal slot images.

use crate::{Checksum, JournalRecord, SLOT_SIZE, decode_slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanError {
    DuplicateSequence(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocatedRecord<const N: usize> {
    pub slot_index: usize,
    pub record: JournalRecord<N>,
}

/// Scans all supplied slots without modifying them.
///
/// Invalid and erased slots are skipped.
/// Any duplicate sequence among valid records is an error.
pub fn find_latest<const N: usize>(
    slots: &[[u8; SLOT_SIZE]],
    checksum: &mut impl Checksum,
) -> Result<Option<LocatedRecord<N>>, ScanError> {
    let mut latest: Option<LocatedRecord<N>> = None;

    for (slot_index, slot) in slots.iter().enumerate() {
        let Ok(record) = decode_slot::<N>(slot, checksum) else {
            continue;
        };

        // Check all previous valid records, including older sequences.
        for previous_slot in &slots[..slot_index] {
            let Ok(previous) = decode_slot::<N>(previous_slot, checksum) else {
                continue;
            };

            if previous.sequence == record.sequence {
                return Err(ScanError::DuplicateSequence(
                    record.sequence,
                ));
            }
        }

        let is_newer = match &latest {
            None => true,
            Some(previous) => record.sequence > previous.record.sequence,
        };

        if is_newer {
            latest = Some(LocatedRecord { slot_index, record });
        }
    }

    Ok(latest)
}

#[cfg(test)]
#[path = "test_journal_scan.rs"]
mod tests;
