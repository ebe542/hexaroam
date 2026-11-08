//! Scans individual slots while retaining record location and sequence.

use crate::{
    CalibrationLoadError, Checksum, JournalRead, LocatedRecord, SLOT_SIZE,
    decode_slot,
};

/// Finds the newest format/CRC-valid record.
///
/// Reads slots individually in ascending order.
/// Invalid records are skipped; storage errors abort scanning.
/// Duplicate sequences among valid records are rejected.
///
/// Missing calibration entries are preserved. Completeness is
/// checked by the application-facing loader, not by this scan.
///
/// Both N and SLOTS must be nonzero.
pub fn scan_calibrations<const N: usize, const SLOTS: usize, S: JournalRead>(
    storage: &mut S,
    checksum: &mut impl Checksum,
) -> Result<Option<LocatedRecord<N>>, CalibrationLoadError<S::Error>> {
    if SLOTS == 0 {
        return Err(CalibrationLoadError::InvalidSlotCount);
    }

    if N == 0 {
        return Err(CalibrationLoadError::InvalidServoCount);
    }

    let mut buffer = [0_u8; SLOT_SIZE];
    let mut sequences = [0_u64; SLOTS];
    let mut valid_count = 0;
    let mut latest: Option<LocatedRecord<N>> = None;

    for slot_index in 0..SLOTS {
        storage
            .read_slot(slot_index, &mut buffer)
            .map_err(CalibrationLoadError::Storage)?;

        let Ok(record) = decode_slot::<N>(&buffer, checksum) else {
            continue;
        };

        if sequences[..valid_count].contains(&record.sequence) {
            return Err(CalibrationLoadError::DuplicateSequence(
                record.sequence,
            ));
        }

        sequences[valid_count] = record.sequence;
        valid_count += 1;

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
#[path = "test_calibration_scan.rs"]
mod tests;
