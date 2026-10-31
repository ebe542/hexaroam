//! Backend contract for controlled servo-signal activation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputState {
    Inactive,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStartError<E> {
    InvalidTargets,
    Backend(E),
}

pub trait OutputBackend {
    type Error;

    /// Validates all prepared initial targets before starting output.
    ///
    /// Ok means the complete output was started.
    /// InvalidTargets must leave output unchanged.
    /// A backend error may change output state; state() must remain valid.
    fn start(&mut self) -> Result<(), OutputStartError<Self::Error>>;

    /// Stops signal output.
    ///
    /// Ok means output is inactive.
    /// After an error, state() must report whether output remains active.
    fn stop(&mut self) -> Result<(), Self::Error>;

    /// Reports the current logical signal-output state.
    ///
    /// This must remain reliable after failed start/stop operations.
    /// It does not report servo position or whether servo power is enabled.
    fn state(&self) -> OutputState;
}
