//! Benchmark-only pulse sink without physical signal output.
//!
//! Active reports the simulated lifecycle state, not physical pulses.
//! This backend must not be used for operating real servos.

use hexaroam_servo_control::{PulseBankLifecycle, PulseBankOutput};
use hexaroam_servo_signal::SignalOutputState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardOutputError {
    AlreadyActive,
    Stopped,
}

#[derive(Default)]
pub struct DiscardPulseOutput {
    active: bool,
}

impl DiscardPulseOutput {
    pub const fn new() -> Self {
        Self { active: false }
    }
}

impl PulseBankOutput for DiscardPulseOutput {
    type Error = DiscardOutputError;

    fn submit_pulses(
        &mut self,
        _pulse_widths_us: &[u32],
    ) -> Result<(), Self::Error> {
        if !self.active {
            return Err(DiscardOutputError::Stopped);
        }

        // ServoBank performs validation and angle-to-pulse conversion.
        // Physical signal generation is deliberately excluded.
        Ok(())
    }

    fn service(&mut self) {}
}

impl PulseBankLifecycle for DiscardPulseOutput {
    type LifecycleError = DiscardOutputError;

    fn start_pulses(
        &mut self,
        _pulse_widths_us: &[u32],
    ) -> Result<(), Self::LifecycleError> {
        if self.active {
            return Err(DiscardOutputError::AlreadyActive);
        }

        self.active = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::LifecycleError> {
        self.active = false;
        Ok(())
    }

    fn state(&self) -> SignalOutputState {
        if self.active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        }
    }
}
