//! Benchmark-only pulse sink without physical signal output.

use core::convert::Infallible;

use hexaroam_servo_control::{PulseBankLifecycle, PulseBankOutput};
use hexaroam_servo_signal::SignalOutputState;

pub struct DiscardPulseOutput;

impl PulseBankOutput for DiscardPulseOutput {
    type Error = Infallible;

    fn submit_pulses(
        &mut self,
        _pulse_widths_us: &[u32],
    ) -> Result<(), Self::Error> {
        // ServoBank performs validation and angle-to-pulse conversion.
        // This benchmark deliberately excludes physical signal generation.
        Ok(())
    }

    fn service(&mut self) {}
}

impl PulseBankLifecycle for DiscardPulseOutput {
    type LifecycleError = Infallible;

    fn start_pulses(
        &mut self,
        _pulse_widths_us: &[u32],
    ) -> Result<(), Self::LifecycleError> {
        // Physical output remains inactive in this benchmark.
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::LifecycleError> {
        Ok(())
    }

    fn state(&self) -> SignalOutputState {
        SignalOutputState::Inactive
    }
}
