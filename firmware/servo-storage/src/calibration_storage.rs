//! Medium-independent access to complete calibration datasets.

use hexaroam_servo_control::Calibration;

/// Read access to validated, complete calibration datasets.
pub trait CalibrationRead<const N: usize> {
    type Error;

    /// Loads the newest valid, complete dataset.
    ///
    /// Returns None when no valid, complete dataset exists.
    /// Storage access errors are returned as Err.
    fn load(&mut self) -> Result<Option<[Calibration; N]>, Self::Error>;
}

/// Write access for the separate calibration station.
pub trait CalibrationStorage<const N: usize>: CalibrationRead<N> {
    /// Saves and verifies one complete dataset.
    ///
    /// Unchanged data must not cause another write.
    /// Success requires readback verification.
    /// Slot selection, rotation and required erasure are internal.
    ///
    /// On error, storage may have been partially modified.
    fn save(
        &mut self,
        calibrations: &[Calibration; N],
    ) -> Result<(), Self::Error>;
}
