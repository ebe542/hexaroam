//! Connects the register-service backend contract to ServoBank.

use hexaroam_servo_control::{
    Angle, PulseBankLifecycle, ServoBank, ServoBankError, ServoId, ServoTarget,
};
use hexaroam_servo_signal::SignalOutputState;

use crate::{
    OutputBackend, OutputStartError, OutputState, TargetReadError,
    TargetWriteError,
};

/// Preserves the distinction between submission and lifecycle failures.
#[derive(Debug, PartialEq, Eq)]
pub enum ServoBankBackendError<Submission, Lifecycle> {
    Submission(Submission),
    Lifecycle(Lifecycle),
}

impl<O: PulseBankLifecycle, const N: usize> OutputBackend for ServoBank<O, N> {
    type Error = ServoBankBackendError<O::Error, O::LifecycleError>;

    fn is_busy_error(error: &Self::Error) -> bool {
        match error {
            ServoBankBackendError::Submission(error) => O::is_busy_error(error),
            ServoBankBackendError::Lifecycle(_) => false,
        }
    }

    fn start(&mut self) -> Result<(), OutputStartError<Self::Error>> {
        match ServoBank::start_prepared(self) {
            Ok(()) => Ok(()),
            Err(ServoBankError::Output(error)) => {
                Err(OutputStartError::Backend(
                    ServoBankBackendError::Lifecycle(error),
                ))
            }
            // Missing or invalid targets prevent the startup call.
            Err(_) => Err(OutputStartError::InvalidTargets),
        }
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        ServoBank::stop(self).map_err(ServoBankBackendError::Lifecycle)
    }

    fn state(&self) -> OutputState {
        match ServoBank::state(self) {
            SignalOutputState::Inactive => OutputState::Inactive,
            SignalOutputState::Active => OutputState::Active,
        }
    }

    fn target(&self, id: ServoId) -> Result<Angle, TargetReadError> {
        match ServoBank::commanded_angle(self, id) {
            Ok(Some(angle)) => Ok(angle),
            Ok(None) => Err(TargetReadError::UninitializedServo(id)),
            Err(ServoBankError::UnknownServo(id)) => {
                Err(TargetReadError::UnknownServo(id))
            }
            // commanded_angle only queries the stored target array.
            // It performs no conversion or output operation.
            Err(_) => unreachable!("Target lookup only checks the servo ID"),
        }
    }

    fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), TargetWriteError<Self::Error>> {
        let result = match ServoBank::state(self) {
            SignalOutputState::Inactive => {
                ServoBank::prepare_targets(self, targets)
            }
            SignalOutputState::Active => ServoBank::set_targets(self, targets),
        };

        result.map_err(|error| match error {
            ServoBankError::UnknownServo(id) => {
                TargetWriteError::UnknownServo(id)
            }
            ServoBankError::DuplicateServo(id) => {
                TargetWriteError::DuplicateServo(id)
            }
            ServoBankError::UninitializedServo(id) => {
                TargetWriteError::UninitializedServo(id)
            }
            ServoBankError::Calibration { id, .. } => {
                TargetWriteError::InvalidAngle(id)
            }
            ServoBankError::Output(error) => TargetWriteError::Backend(
                ServoBankBackendError::Submission(error),
            ),
        })
    }

    fn service(&mut self) {
        ServoBank::service(self);
    }
}
