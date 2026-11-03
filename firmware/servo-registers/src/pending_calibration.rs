//! Owned, complete calibration data awaiting persistence.

use hexaroam_servo_control::{Calibration, ServoId};
use hexaroam_servo_storage::{Checksum, FormatError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingCalibrationError {
    Format(FormatError),
    MissingServo(ServoId),
}

#[derive(Debug, PartialEq, Eq)]
pub struct PendingCalibration<const N: usize> {
    calibrations: [Option<Calibration>; N],
}

impl<const N: usize> PendingCalibration<N> {
    /// Decodes a complete calibration set into owned RAM storage.
    ///
    /// Missing entries are rejected in ascending ServoId order.
    /// No partially validated request is returned.
    pub fn from_bytes(
        input: &[u8],
        checksum: &mut impl Checksum,
    ) -> Result<Self, PendingCalibrationError> {
        let calibrations = hexaroam_servo_storage::decode::<N>(input, checksum)
            .map_err(PendingCalibrationError::Format)?;

        if let Some(index) = calibrations
            .iter()
            .position(Option::is_none)
        {
            return Err(PendingCalibrationError::MissingServo(
                ServoId(index),
            ));
        }

        Ok(Self { calibrations })
    }

    pub fn calibrations(&self) -> &[Option<Calibration>; N] {
        &self.calibrations
    }
}

#[cfg(test)]
#[path = "test_pending_calibration.rs"]
mod tests;
