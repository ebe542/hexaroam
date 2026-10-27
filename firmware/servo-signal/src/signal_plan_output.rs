//! Submission and servicing of complete signal plans.

use crate::SignalPlan;

pub trait SignalPlanOutput {
    type Error;

    /// Accepts the complete plan or rejects it without partial acceptance.
    ///
    /// The implementation must copy or encode the borrowed plan before
    /// returning. Success does not confirm physical activation.
    fn submit_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::Error>;

    /// Services the output without waiting for a complete signal period.
    fn service(&mut self);
}
