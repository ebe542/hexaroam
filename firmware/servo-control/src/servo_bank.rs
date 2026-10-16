//! Validated commands for a configured collection of servos.

use crate::{Angle, CalibrationError, ServoChannel, ServoError};

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
    Calibration {
        id: ServoId,
        error: CalibrationError,
    },
    Output {
        id: ServoId,
        error: ServoError<E>,
    },
}

pub struct ServoBank<'a, E, const N: usize> {
    channels: [&'a mut dyn ServoChannel<Error = E>; N],
}

impl<'a, E, const N: usize> ServoBank<'a, E, N> {
    pub fn new(channels: [&'a mut dyn ServoChannel<Error = E>; N]) -> Self {
        Self { channels }
    }

    /// Validates every target before issuing any output command.
    ///
    /// Validation errors leave all outputs untouched.
    /// Output commands follow the supplied order and stop at the
    /// first error. Previously accepted commands are not rolled back.
    pub fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), ServoBankError<E>> {
        // Validate the entire batch before submitting any command.
        for (index, target) in targets.iter().enumerate() {
            let channel = self
                .channels
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

            channel
                .validate_angle(target.angle)
                .map_err(|error| ServoBankError::Calibration {
                    id: target.id,
                    error,
                })?;
        }

        // Submit only after every target has passed validation.
        for target in targets {
            self.channels[target.id.0]
                .set_angle(target.angle)
                .map_err(|error| ServoBankError::Output {
                    id: target.id,
                    error,
                })?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "test_servo_bank.rs"]
mod tests;
