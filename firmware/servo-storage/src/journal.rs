//! Appends complete calibration records to the two-sector journal.

use hexaroam_servo_control::Calibration;

use crate::{
    Checksum, JournalRecord, JournalStorage, LocatedRecord, SECTOR_COUNT,
    SLOT_COUNT, SLOT_SIZE, SLOTS_PER_SECTOR, ScanError, SlotError, encode_slot,
    find_latest,
};

#[derive(Debug, PartialEq, Eq)]
pub enum JournalError<E> {
    Storage(E),
    Scan(ScanError),
    Slot(SlotError),
    SequenceOverflow,
    VerificationFailed,
}

/// Appends and verifies one complete calibration record.
///
/// The previous valid record is preserved during sector changes.
/// Each call reads the storage state again before choosing a slot.
/// Storage errors may have partially modified the target slot/sector.
pub fn append<const N: usize, S: JournalStorage>(
    storage: &mut S,
    calibrations: &[Option<Calibration>; N],
    checksum: &mut impl Checksum,
) -> Result<LocatedRecord<N>, JournalError<S::Error>> {
    // Read the complete journal before choosing or modifying storage.
    let mut slots = [[0xFF; SLOT_SIZE]; SLOT_COUNT];

    for (index, slot) in slots.iter_mut().enumerate() {
        storage
            .read_slot(index, slot)
            .map_err(JournalError::Storage)?;
    }

    let latest =
        find_latest::<N>(&slots, checksum).map_err(JournalError::Scan)?;

    let (sequence, active_sector) = match latest {
        Some(latest) => {
            let sequence = latest
                .record
                .sequence
                .checked_add(1)
                .ok_or(JournalError::SequenceOverflow)?;

            (
                sequence,
                latest.slot_index / SLOTS_PER_SECTOR,
            )
        }
        None => (1, 0),
    };

    // Validate and encode before any erase or write operation.
    let mut candidate = [0xFF; SLOT_SIZE];

    encode_slot(
        sequence,
        calibrations,
        &mut candidate,
        checksum,
    )
    .map_err(JournalError::Slot)?;

    let is_erased =
        |slot: &[u8; SLOT_SIZE]| slot.iter().all(|&byte| byte == 0xFF);

    let active_start = active_sector * SLOTS_PER_SECTOR;
    let active_end = active_start + SLOTS_PER_SECTOR;

    // Partially programmed slots count as occupied.
    // Do not reuse erased holes before the last occupied slot.
    let next_slot = slots[active_start..active_end]
        .iter()
        .rposition(|slot| !is_erased(slot))
        .map_or(active_start, |index| {
            active_start + index + 1
        });

    let target_slot = if next_slot < active_end {
        next_slot
    } else {
        let other_sector = (active_sector + 1) % SECTOR_COUNT;
        let other_start = other_sector * SLOTS_PER_SECTOR;
        let other_end = other_start + SLOTS_PER_SECTOR;

        if slots[other_start..other_end]
            .iter()
            .any(|slot| !is_erased(slot))
        {
            storage
                .erase_sector(other_sector)
                .map_err(JournalError::Storage)?;
        }

        other_start
    };

    storage
        .write_slot(target_slot, &candidate)
        .map_err(JournalError::Storage)?;

    let mut readback = [0; SLOT_SIZE];

    storage
        .read_slot(target_slot, &mut readback)
        .map_err(JournalError::Storage)?;

    if readback != candidate {
        return Err(JournalError::VerificationFailed);
    }

    Ok(LocatedRecord {
        slot_index: target_slot,
        record: JournalRecord {
            sequence,
            calibrations: *calibrations,
        },
    })
}

#[cfg(test)]
#[path = "test_journal.rs"]
mod tests;
