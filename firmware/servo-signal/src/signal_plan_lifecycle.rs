//! Explicit start and stop of signal-plan output.

use crate::{SignalPlan, SignalPlanOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalOutputState {
    /// Output is stopped and all assigned signal pins are Low.
    Inactive,

    /// Output is enabled or a completed stop has not been confirmed.
    ///
    /// This does not mean that any pin is currently High.
    Active,
}

/// Optional lifecycle support for a signal-plan output backend.
///
/// Lifecycle errors are separate from ordinary plan-submission errors.
/// Implementations retain ownership of all resources after errors.
pub trait SignalPlanLifecycle: SignalPlanOutput {
    type LifecycleError;

    /// Starts inactive output with one complete initial plan.
    ///
    /// The complete plan must be validated before enabling output.
    /// The borrowed plan must be copied or encoded before returning.
    ///
    /// Starting active output must be rejected without changing it.
    /// Invalid plans leave inactive output unchanged.
    /// A latched fault must not be cleared by an ordinary start.
    ///
    /// Success confirms startup, not a measured pulse or servo position.
    /// After a technical error, state() must remain reliable.
    fn start_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::LifecycleError>;

    /// Stops output and discards any pending replacement plan.
    ///
    /// A pulse in progress may be shortened.
    /// Success guarantees all assigned signal pins are Low and DMA
    /// no longer accesses the output buffers.
    ///
    /// Stopping already inactive output succeeds without restarting it.
    /// Stopping does not clear a latched fault.
    ///
    /// On failure, resources and buffers remain owned by the backend.
    /// Inactive must not be reported until stopping is confirmed.
    fn stop(&mut self) -> Result<(), Self::LifecycleError>;

    /// Returns the last confirmed logical output state.
    ///
    /// This does not report servo power or mechanical position.
    fn state(&self) -> SignalOutputState;
}
