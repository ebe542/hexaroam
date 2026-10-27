//! Transactional angle commands for a configured servo bank.

use crate::{Angle, Calibration, CalibrationError};

pub trait PulseBankOutput {
    type Error;

    /// Submits all servo pulse widths in ServoId order.
    ///
    /// The backend must consume or copy the slice during this call.
    /// Failure must not accept any part of the new batch.
    /// Success confirms acceptance, not physical activation.
    fn submit_pulses(
        &mut self,
        pulse_widths_us: &[u32],
    ) -> Result<(), Self::Error>;

    /// Services the backend without submitting new target values.
    fn service(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServoId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServoTarget {
    pub id: ServoId,
    pub angle: Angle,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServoBankError<E> {
    UnknownServo(ServoId),
    DuplicateServo(ServoId),
    UninitializedServo(ServoId),
    Calibration {
        id: ServoId,
        error: CalibrationError,
    },
    Output(E),
}

pub struct ServoBank<O: PulseBankOutput, const N: usize> {
    calibrations: [Calibration; N],
    commanded_angles: [Option<Angle>; N],
    output: O,
}

impl<O: PulseBankOutput, const N: usize> ServoBank<O, N> {
    /// Constructs the bank without submitting output.
    pub fn new(calibrations: [Calibration; N], output: O) -> Self {
        Self {
            calibrations,
            commanded_angles: [None; N],
            output,
        }
    }

    /// Returns the last accepted command, not a measured position.
    pub fn commanded_angle(
        &self,
        id: ServoId,
    ) -> Result<Option<Angle>, ServoBankError<O::Error>> {
        self.commanded_angles
            .get(id.0)
            .copied()
            .ok_or(ServoBankError::UnknownServo(id))
    }

    /// Validates and submits one complete pulse batch.
    ///
    /// The first nonempty request must initialize all servos.
    /// Later requests may change a subset.
    /// Empty requests succeed without output.
    /// Errors preserve all previously accepted angle commands.
    pub fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), ServoBankError<O::Error>> {
        if targets.is_empty() {
            return Ok(());
        }

        // Work on a local candidate until the complete batch is accepted.
        let mut candidate_angles = self.commanded_angles;

        for (index, target) in targets.iter().enumerate() {
            let calibration = self
                .calibrations
                .get(target.id.0)
                .ok_or(ServoBankError::UnknownServo(target.id))?;

            if targets[..index]
                .iter()
                .any(|previous| previous.id == target.id)
            {
                return Err(ServoBankError::DuplicateServo(
                    target.id,
                ));
            }

            calibration
                .pulse_width_us(target.angle)
                .map_err(|error| ServoBankError::Calibration {
                    id: target.id,
                    error,
                })?;

            candidate_angles[target.id.0] = Some(target.angle);
        }

        let mut pulse_widths_us = [0_u32; N];

        // Every servo needs a known angle to produce a complete batch.
        for (index, candidate) in candidate_angles.iter().enumerate() {
            let id = ServoId(index);
            let angle =
                candidate.ok_or(ServoBankError::UninitializedServo(id))?;

            pulse_widths_us[index] = self.calibrations[index]
                .pulse_width_us(angle)
                .map_err(|error| ServoBankError::Calibration { id, error })?;
        }

        self.output
            .submit_pulses(&pulse_widths_us)
            .map_err(ServoBankError::Output)?;

        // Commit only after the backend accepted the complete batch.
        self.commanded_angles = candidate_angles;

        Ok(())
    }

    pub fn service(&mut self) {
        self.output.service();
    }
}

#[cfg(test)]
#[path = "test_servo_bank.rs"]
mod tests;
