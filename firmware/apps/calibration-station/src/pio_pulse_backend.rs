//! Single-channel adapter for the calibration-station pulse service.
//!
//! Uses GP0, PIO0 state machine 0 and DMA channel 0.
//! Hardware initialization remains the application's responsibility.

use crate::control::{BackendError, PulseBackend};
use hexaroam_servo_pio::{OutputSubmissionError, SwitchingOutput};
use hexaroam_servo_signal::{
    PulseLimits, PulseTarget, SignalOutputState, SignalPlan,
};
use rp_pico::hal::{dma::CH0, pac::PIO0, pio::SM0};

pub const SERVO_GPIO: u8 = 0;
pub const PERIOD_US: u32 = 20_000;

// One channel needs two plan entries, each encoded as two words.
pub type SingleServoOutput = SwitchingOutput<PIO0, SM0, CH0, 1, 4>;

pub struct PioPulseBackend {
    output: SingleServoOutput,
    limits: PulseLimits,
}

impl PioPulseBackend {
    /// Takes ownership of an initialized, inactive output.
    ///
    /// The output must be configured for SERVO_GPIO and PERIOD_US.
    /// Construction does not start PIO or DMA.
    pub fn new(
        mut output: SingleServoOutput,
        limits: PulseLimits,
    ) -> Result<Self, BackendError> {
        if output.state() != SignalOutputState::Inactive || output.is_faulted()
        {
            return Err(BackendError::OutputError);
        }

        Ok(Self { output, limits })
    }

    fn signal_plan(
        &self,
        pulse_width_us: u32,
    ) -> Result<SignalPlan<2>, BackendError> {
        SignalPlan::new(
            &[PulseTarget {
                gpio: SERVO_GPIO,
                pulse_width_us,
            }],
            &self.limits,
            PERIOD_US,
        )
        .map_err(|_| BackendError::OutputError)
    }
}

impl PulseBackend for PioPulseBackend {
    fn state(&self) -> SignalOutputState {
        self.output.state()
    }

    fn is_faulted(&mut self) -> bool {
        self.output.is_faulted()
    }

    fn start(&mut self, pulse_width_us: u32) -> Result<(), BackendError> {
        let plan = self.signal_plan(pulse_width_us)?;

        self.output
            .start_plan(&plan)
            .map_err(|_| BackendError::OutputError)
    }

    fn set_pulse(&mut self, pulse_width_us: u32) -> Result<(), BackendError> {
        let plan = self.signal_plan(pulse_width_us)?;

        self.output
            .submit_plan(&plan)
            .map_err(|error| match error {
                OutputSubmissionError::Busy => BackendError::Busy,
                _ => BackendError::OutputError,
            })
    }

    fn stop(&mut self) -> Result<(), BackendError> {
        self.output
            .stop()
            .map_err(|_| BackendError::OutputError)
    }

    fn service(&mut self) {
        self.output.service();
    }
}
