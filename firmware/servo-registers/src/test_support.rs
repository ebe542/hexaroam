use crate::{OutputBackend, OutputStartError, OutputState};

pub(crate) struct FakeOutput {
    pub(crate) state: OutputState,
    pub(crate) start_calls: usize,
    pub(crate) stop_calls: usize,
    pub(crate) start_error: Option<OutputStartError<()>>,
    pub(crate) stop_error: bool,
    pub(crate) state_after_error: OutputState,
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
}
