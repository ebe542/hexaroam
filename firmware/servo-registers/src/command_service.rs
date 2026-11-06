//! Controller state for the command protocol.
//!
//! Independent of COBS, CRC and UART. Calibration is fixed at boot.

use crate::{OutputBackend, OutputStartError, OutputState, TargetWriteError};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_protocol::command::{NackCode, TargetBatch};

pub const CALIBRATION_VALID: u32 = 1 << 0;
pub const OUTPUT_ENABLED: u32 = 1 << 1;
pub const RUNNING: u32 = 1 << 2;
pub const OUTPUT_ERROR: u32 = 1 << 8;

pub struct CommandService<O: OutputBackend, const N: usize> {
    output: O,
    flags: u32,
}

impl<O: OutputBackend, const N: usize> CommandService<O, N> {
    /// Takes ownership of the backend without starting output.
    ///
    /// Normal boot supplies an inactive backend. The status always
    /// reports the backend's actual state, independently of permission.
    pub fn new(output: O, calibration_valid: bool) -> Self {
        Self {
            output,
            flags: if calibration_valid {
                CALIBRATION_VALID
            } else {
                0
            },
        }
    }

    pub fn status(&self) -> u32 {
        let running = if self.output.state() == OutputState::Active {
            RUNNING
        } else {
            0
        };

        self.flags | running
    }

    /// Enable only grants permission; it never starts output.
    ///
    /// Disable revokes permission immediately and stops active output.
    /// Failed stop latches OUTPUT_ERROR. A successful later stop does
    /// not clear the error. Repeated enable/disable is idempotent.
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), NackCode> {
        if enabled {
            if self.flags & OUTPUT_ERROR != 0 {
                return Err(NackCode::OutputError);
            }

            if self.flags & CALIBRATION_VALID == 0 {
                return Err(NackCode::InvalidState);
            }

            self.flags |= OUTPUT_ENABLED;
            return Ok(());
        }

        self.flags &= !OUTPUT_ENABLED;

        if self.output.state() == OutputState::Active {
            if self.output.stop().is_err() {
                self.flags |= OUTPUT_ERROR;
                return Err(NackCode::OutputError);
            }
        }

        Ok(())
    }

    /// Rejected batches and Busy preserve accepted targets and permission.
    /// Initial targets are prepared before startup. If startup fails, those
    /// prepared values may remain stored, but another complete initial
    /// batch is required for the next attempt.
    ///
    /// Technical failures latch OUTPUT_ERROR, revoke permission and attempt
    /// shutdown. RUNNING continues to reflect the confirmed backend state.
    pub fn set_targets(
        &mut self,
        batch: TargetBatch<'_>,
    ) -> Result<(), NackCode> {
        if self.flags & OUTPUT_ERROR != 0 {
            return Err(NackCode::OutputError);
        }

        if self.flags & CALIBRATION_VALID == 0
            || self.flags & OUTPUT_ENABLED == 0
        {
            return Err(NackCode::InvalidState);
        }

        let first = usize::from(batch.first_servo);
        let count = batch.angle_count();

        let end = first
            .checked_add(count)
            .ok_or(NackCode::InvalidValue)?;

        if count == 0 || first >= N || end > N {
            return Err(NackCode::InvalidValue);
        }

        let starting = self.output.state() == OutputState::Inactive;

        if starting && (first != 0 || count != N) {
            return Err(NackCode::InvalidState);
        }

        let mut targets = [ServoTarget {
            id: ServoId(0),
            angle: Angle::from_centidegrees(0),
        }; N];

        for (index, target) in targets[..count].iter_mut().enumerate() {
            let angle = batch
                .angle(index)
                .ok_or(NackCode::InvalidLength)?;

            *target = ServoTarget {
                id: ServoId(first + index),
                angle: Angle::from_centidegrees(angle),
            };
        }

        // The backend validates the complete batch before accepting it.
        match self
            .output
            .set_targets(&targets[..count])
        {
            Ok(()) => {}
            Err(
                TargetWriteError::UnknownServo(_)
                | TargetWriteError::DuplicateServo(_)
                | TargetWriteError::InvalidAngle(_),
            ) => {
                return Err(NackCode::InvalidValue);
            }
            Err(TargetWriteError::UninitializedServo(_)) => {
                return Err(NackCode::InvalidState);
            }
            Err(TargetWriteError::Backend(error)) => {
                if O::is_busy_error(&error) {
                    return Err(NackCode::Busy);
                }

                return Err(self.output_failed());
            }
        }

        if starting {
            match self.output.start() {
                Ok(()) => {}
                Err(OutputStartError::InvalidTargets) => {
                    return Err(NackCode::InvalidValue);
                }
                Err(OutputStartError::Backend(_)) => {
                    return Err(self.output_failed());
                }
            }
        }

        Ok(())
    }

    /// Latches a technical failure and attempts to stop active output.
    /// A failed shutdown remains visible through the backend's state.
    fn output_failed(&mut self) -> NackCode {
        self.flags |= OUTPUT_ERROR;
        self.flags &= !OUTPUT_ENABLED;

        if self.output.state() == OutputState::Active {
            // Preserve the original failure. The latched error and actual
            // output state remain available even if shutdown also fails.
            let _ = self.output.stop();
        }

        NackCode::OutputError
    }

    /// Services ongoing output without enabling, starting or recovering it.
    /// Must remain callable while disabled or after a latched error.
    pub fn service(&mut self) {
        self.output.service();
    }
}

#[cfg(test)]
#[path = "test_command_service.rs"]
mod tests;

#[cfg(test)]
#[path = "test_command_targets.rs"]
mod target_tests;

#[cfg(test)]
#[path = "test_command_integration.rs"]
mod integration_tests;
