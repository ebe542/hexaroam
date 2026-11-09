//! Test-only register backend without physical servo outputs.

use core::convert::Infallible;

use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_registers::{
    OutputBackend, OutputStartError, OutputState, TargetReadError,
    TargetWriteError,
};

pub struct NoServoOutput;

impl OutputBackend for NoServoOutput {
    type Error = Infallible;

    fn start(&mut self) -> Result<(), OutputStartError<Self::Error>> {
        Err(OutputStartError::InvalidTargets)
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn state(&self) -> OutputState {
        OutputState::Inactive
    }

    fn target(&self, id: ServoId) -> Result<Angle, TargetReadError> {
        Err(TargetReadError::UnknownServo(id))
    }

    fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), TargetWriteError<Self::Error>> {
        match targets.first() {
            Some(target) => Err(TargetWriteError::UnknownServo(
                target.id,
            )),
            None => Ok(()),
        }
    }

    fn service(&mut self) {}
}
