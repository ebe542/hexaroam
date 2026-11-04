//! Connects released calibration requests to journal persistence.

use crate::{CalibrationCompletionError, FrameProcessor, OutputBackend};
use hexaroam_servo_storage::{Checksum, JournalError, JournalStorage};

#[derive(Debug, PartialEq, Eq)]
pub enum CalibrationPersistenceError<E> {
    Journal(JournalError<E>),
    Completion(CalibrationCompletionError),
}

/// Persists one released request and reports its result.
///
/// Ok(false): no request was ready; storage was not accessed.
/// Ok(true): persistence succeeded, including unchanged journal data.
/// Err: persistence or completion failed; journal errors remain available.
pub fn persist_calibration<O: OutputBackend, S: JournalStorage>(
    processor: &mut FrameProcessor<O>,
    storage: &mut S,
    checksum: &mut impl Checksum,
) -> Result<bool, CalibrationPersistenceError<S::Error>> {
    let Some(pending) = processor.calibration_ready() else {
        return Ok(false);
    };

    let saved = hexaroam_servo_storage::save_if_changed(
        storage,
        pending.calibrations(),
        checksum,
    );

    let result = match &saved {
        Ok(_) => crate::CalibrationSaveResult::Success,
        Err(JournalError::VerificationFailed) => {
            crate::CalibrationSaveResult::VerificationFailed
        }
        Err(_) => crate::CalibrationSaveResult::StorageError,
    };

    processor
        .complete_calibration(result)
        .map_err(CalibrationPersistenceError::Completion)?;

    saved
        .map(|_| true)
        .map_err(CalibrationPersistenceError::Journal)
}
