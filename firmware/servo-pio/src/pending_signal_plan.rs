//! Preparation of the next complete output plan.

use hexaroam_servo_signal::SignalPlan;

use crate::{OutputBuffer, encoding::EncodingError};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SubmissionError {
    Busy,
    PeriodMismatch,
    OutputMismatch,
    Encoding(EncodingError),
}

pub(crate) struct PendingSignalPlan<'a, const WORD_CAPACITY: usize> {
    buffer: &'a mut OutputBuffer<WORD_CAPACITY>,
    period_us: u32,
    gpio_mask: u32,
    ready: bool,
}

impl<'a, const WORD_CAPACITY: usize> PendingSignalPlan<'a, WORD_CAPACITY> {
    pub(crate) fn new(
        buffer: &'a mut OutputBuffer<WORD_CAPACITY>,
        period_us: u32,
        gpio_mask: u32,
    ) -> Self {
        Self {
            buffer,
            period_us,
            gpio_mask,
            ready: false,
        }
    }

    pub(crate) fn submit<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<(), SubmissionError> {
        if self.ready {
            return Err(SubmissionError::Busy);
        }

        if plan.period_us() != self.period_us {
            return Err(SubmissionError::PeriodMismatch);
        }

        // Every configured output starts High in a complete SignalPlan.
        let initial_mask = plan
            .entries()
            .first()
            .map(|entry| entry.gpio_mask);

        if initial_mask != Some(self.gpio_mask) {
            return Err(SubmissionError::OutputMismatch);
        }

        self.buffer
            .load(plan, self.gpio_mask)
            .map_err(SubmissionError::Encoding)?;

        self.ready = true;

        Ok(())
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.ready
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        &'a mut OutputBuffer<WORD_CAPACITY>,
        bool,
    ) {
        (self.buffer, self.ready)
    }
}

#[cfg(test)]
#[path = "test_pending_signal_plan.rs"]
mod tests;
