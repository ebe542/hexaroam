//! Loads complete calibration datasets by scanning slots individually.

use hexaroam_servo_control::Calibration;

use crate::{Checksum, JournalRead, scan_calibrations};

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
    let Some(latest) = scan_calibrations::<N, SLOTS, _>(storage, checksum)?
    else {
        return Ok(None);
    };

    // The scan rejects N == 0 before accessing storage.
    let first = latest.record.calibrations[0]
        .ok_or(CalibrationLoadError::IncompleteCalibration(0))?;

    let mut calibrations = [first; N];

    for (index, entry) in latest
        .record
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
