use crate::{
    OutputBackend, OutputStartError, OutputState, TargetReadError,
    TargetWriteError,
};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};

pub(crate) struct FakeOutput {
    pub(crate) state: OutputState,
    pub(crate) start_calls: usize,
    pub(crate) stop_calls: usize,
    pub(crate) start_error: Option<OutputStartError<()>>,
    pub(crate) stop_error: bool,
    pub(crate) state_after_error: OutputState,
    pub(crate) targets: [Angle; 18],
    pub(crate) target_error: bool,
}

impl FakeOutput {
    pub(crate) fn inactive() -> Self {
        Self {
            state: OutputState::Inactive,
            start_calls: 0,
            stop_calls: 0,
            start_error: None,
            stop_error: false,
            state_after_error: OutputState::Inactive,
            targets: [Angle::from_centidegrees(0); 18],
            target_error: false,
        }
    }
}

impl OutputBackend for FakeOutput {
    type Error = ();

    fn start(&mut self) -> Result<(), OutputStartError<Self::Error>> {
        self.start_calls += 1;

        if let Some(error) = self.start_error {
            // InvalidTargets must not change output state.
            if matches!(error, OutputStartError::Backend(_)) {
                self.state = self.state_after_error;
            }
            return Err(error);
        }

        self.state = OutputState::Active;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        self.stop_calls += 1;

        if self.stop_error {
            self.state = self.state_after_error;
            return Err(());
        }

        self.state = OutputState::Inactive;
        Ok(())
    }

    fn state(&self) -> OutputState {
        self.state
    }

    fn target(&self, id: ServoId) -> Result<Angle, TargetReadError> {
        self.targets
            .get(id.0)
            .copied()
            .ok_or(TargetReadError::UnknownServo(id))
    }

    fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), TargetWriteError<Self::Error>> {
        let mut candidate = self.targets;

        for (index, target) in targets.iter().enumerate() {
            if target.id.0 >= candidate.len() {
                return Err(TargetWriteError::UnknownServo(
                    target.id,
                ));
            }

            if targets[..index]
                .iter()
                .any(|previous| previous.id == target.id)
            {
                return Err(TargetWriteError::DuplicateServo(
                    target.id,
                ));
            }

            if !(-9_000..=9_000).contains(&target.angle.as_centidegrees()) {
                return Err(TargetWriteError::InvalidAngle(
                    target.id,
                ));
            }

            candidate[target.id.0] = target.angle;
        }

        if self.target_error {
            return Err(TargetWriteError::Backend(()));
        }

        self.targets = candidate;
        Ok(())
    }
}
