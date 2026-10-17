//! Storage for a completely encoded output plan.

use hexaroam_servo_signal::SignalPlan;

use crate::encoding::{EncodedSignalPlan, EncodingError};

pub struct OutputBuffer<const WORD_CAPACITY: usize> {
    encoded: Option<EncodedSignalPlan<WORD_CAPACITY>>,
}

impl<const WORD_CAPACITY: usize> OutputBuffer<WORD_CAPACITY> {
    pub const fn new() -> Self {
        Self { encoded: None }
    }

    pub(crate) fn load<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
        allowed_gpio_mask: u32,
    ) -> Result<(), EncodingError> {
        let encoded = EncodedSignalPlan::new(plan, allowed_gpio_mask)?;

        self.encoded = Some(encoded);

        Ok(())
    }

    pub(crate) fn words(&self) -> &[u32] {
        match &self.encoded {
            Some(encoded) => encoded.words(),
            None => &[],
        }
    }
}

#[cfg(test)]
#[path = "test_buffer.rs"]
mod tests;
