//! Backend contract for controlled servo-signal activation.

use hexaroam_servo_control::{Angle, ServoId, ServoTarget};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetReadError {
    UnknownServo(ServoId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetWriteError<E> {
    UnknownServo(ServoId),
    DuplicateServo(ServoId),
    InvalidAngle(ServoId),
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

    /// Returns the last accepted target, not a measured position.
    fn target(&self, id: ServoId) -> Result<Angle, TargetReadError>;

    /// Validates and accepts the complete target batch.
    ///
    /// Inactive output: prepare targets without starting output.
    /// Active output: submit the complete updated signal plan.
    ///
    /// The slice must be consumed or copied during this call.
    /// Failure preserves all previously accepted targets.
    /// Success confirms acceptance, not physical activation.
    fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), TargetWriteError<Self::Error>>;
}
