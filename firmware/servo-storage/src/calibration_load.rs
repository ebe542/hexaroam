//! Loads complete calibration datasets by scanning slots individually.

use hexaroam_servo_control::Calibration;

use crate::{Checksum, JournalRead, JournalRecord, SLOT_SIZE, decode_slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationLoadError<E> {
    InvalidSlotCount,
    InvalidServoCount,
    Storage(E),
    DuplicateSequence(u64),
    IncompleteCalibration(usize),
}

/// Loads the newest valid record and requires complete calibration.
///
/// Reads each configured slot exactly once, in ascending order.
/// Invalid records are skipped. Storage errors abort loading.
/// Duplicate sequences among valid records are rejected.
///
/// Completeness is checked after selecting the newest valid record.
/// An incomplete newest record must not cause fallback to older data.
///
/// Both N and SLOTS must be nonzero. Invalid configuration is
/// rejected before storage access.
pub fn load_calibrations<const N: usize, const SLOTS: usize, S: JournalRead>(
    storage: &mut S,
    checksum: &mut impl Checksum,
) -> Result<Option<[Calibration; N]>, CalibrationLoadError<S::Error>> {
    if SLOTS == 0 {
        return Err(CalibrationLoadError::InvalidSlotCount);
    }

    if N == 0 {
        return Err(CalibrationLoadError::InvalidServoCount);
    }

    let mut buffer = [0_u8; SLOT_SIZE];
    let mut sequences = [0_u64; SLOTS];
    let mut valid_count = 0;
    let mut latest: Option<JournalRecord<N>> = None;

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
            Some(previous) => record.sequence > previous.sequence,
        };

        if is_newer {
            latest = Some(record);
        }
    }

    let Some(record) = latest else {
        return Ok(None);
    };

    // N was checked above. Use the first calibration to initialize
    // the complete output array without placeholders or unwrap().
    let first = record.calibrations[0]
        .ok_or(CalibrationLoadError::IncompleteCalibration(0))?;

    let mut calibrations = [first; N];

    for (index, entry) in record
        .calibrations
        .into_iter()
        .enumerate()
    {
        calibrations[index] =
            entry.ok_or(CalibrationLoadError::IncompleteCalibration(index))?;
    }

    Ok(Some(calibrations))
}

#[cfg(test)]
#[path = "test_calibration_load.rs"]
mod tests;
