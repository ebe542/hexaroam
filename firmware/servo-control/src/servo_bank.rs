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

    /// Returns the last accepted target, prepared or submitted.
    ///
    /// This does not report measured position or signal-plan activation.
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
    /// Every servo must have a target after applying this request.
    /// Previously prepared or submitted targets fill omitted entries.
    /// Empty requests succeed without output.
    /// Errors preserve all previously accepted angle commands.
    pub fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), ServoBankError<O::Error>> {
        if targets.is_empty() {
            return Ok(());
        }

        let candidate = self.validated_targets(targets)?;
        let pulses = self.pulse_widths(&candidate)?;

        self.output
            .submit_pulses(&pulses)
            .map_err(ServoBankError::Output)?;

        // Commit only after the complete batch was accepted.
        self.commanded_angles = candidate;

        Ok(())
    }

    pub fn service(&mut self) {
        self.output.service();
    }

    /// Validates and stores targets without submitting signal output.
    ///
    /// Partial initialization is allowed.
    /// Invalid requests preserve all previously accepted targets.
    ///
    /// The caller must ensure that signal output is inactive.
    pub fn prepare_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), ServoBankError<O::Error>> {
        let candidate = self.validated_targets(targets)?;

        self.commanded_angles = candidate;

        Ok(())
    }

    /// Submits all previously accepted targets as one pulse batch.
    ///
    /// Every servo must have a valid target.
    /// This does not change the stored targets.
    /// Success confirms acceptance, not physical activation.
    pub fn submit_prepared(&mut self) -> Result<(), ServoBankError<O::Error>> {
        let pulses = self.pulse_widths(&self.commanded_angles)?;

        self.output
            .submit_pulses(&pulses)
            .map_err(ServoBankError::Output)
    }

    /// Builds a validated candidate without changing accepted targets.
    fn validated_targets(
        &self,
        targets: &[ServoTarget],
    ) -> Result<[Option<Angle>; N], ServoBankError<O::Error>> {
        let mut candidate = self.commanded_angles;

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

            candidate[target.id.0] = Some(target.angle);
        }

        Ok(candidate)
    }

    /// Converts a complete candidate into servo-ID-ordered pulse widths.
    fn pulse_widths(
        &self,
        angles: &[Option<Angle>; N],
    ) -> Result<[u32; N], ServoBankError<O::Error>> {
        let mut pulses = [0; N];

        for (index, candidate) in angles.iter().enumerate() {
            let id = ServoId(index);
            let angle =
                candidate.ok_or(ServoBankError::UninitializedServo(id))?;

            pulses[index] = self.calibrations[index]
                .pulse_width_us(angle)
                .map_err(|error| ServoBankError::Calibration { id, error })?;
        }

        Ok(pulses)
    }
}

#[cfg(test)]
#[path = "test_servo_bank.rs"]
mod tests;
