//! Raw-pulse control for the calibration station.

use hexaroam_servo_signal::{PulseLimits, SignalOutputState};

pub const PULSE_READY: u32 = 1 << 0;
pub const RUNNING: u32 = 1 << 1;
pub const OUTPUT_ERROR: u32 = 1 << 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendError {
    Busy,
    OutputError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    OutOfRange,
    MissingPulse,
    Busy,
    OutputError,
}

/// Owns the output hardware. Construction must not start output.
/// Rejected pulse updates must preserve the previously accepted plan.
pub trait PulseBackend {
    fn state(&self) -> SignalOutputState;
    fn is_faulted(&mut self) -> bool;
    fn start(&mut self, pulse_width_us: u32) -> Result<(), BackendError>;
    fn set_pulse(&mut self, pulse_width_us: u32) -> Result<(), BackendError>;
    fn stop(&mut self) -> Result<(), BackendError>;
    fn service(&mut self);
}

pub struct CalibrationControl<B: PulseBackend> {
    backend: B,
    limits: PulseLimits,
    pulse_width_us: Option<u32>,
    flags: u32,
}

impl<B: PulseBackend> CalibrationControl<B> {
    pub fn new(mut backend: B, limits: PulseLimits) -> Self {
        let flags = if backend.is_faulted() {
            OUTPUT_ERROR
        } else {
            0
        };

        Self {
            backend,
            limits,
            pulse_width_us: None,
            flags,
        }
    }

    pub fn status(&self) -> u32 {
        let running = if self.backend.state() == SignalOutputState::Active {
            RUNNING
        } else {
            0
        };

        self.flags | running
    }

    pub fn pulse_width_us(&self) -> Option<u32> {
        self.pulse_width_us
    }

    pub fn set_pulse(
        &mut self,
        pulse_width_us: u32,
    ) -> Result<(), ControlError> {
        if !self.limits.contains(pulse_width_us) {
            return Err(ControlError::OutOfRange);
        }

        self.check_output_fault()?;

        if self.backend.state() == SignalOutputState::Active {
            self.backend
                .set_pulse(pulse_width_us)
                .map_err(|error| self.record_error(error))?;
        }

        // Only commit the value after validation and backend acceptance.
        self.pulse_width_us = Some(pulse_width_us);
        self.flags |= PULSE_READY;

        Ok(())
    }

    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), ControlError> {
        if !enabled {
            // Stopping remains possible after a latched output error.
            return self
                .backend
                .stop()
                .map_err(|error| self.record_error(error));
        }

        self.check_output_fault()?;

        if self.backend.state() == SignalOutputState::Active {
            return Ok(());
        }

        let pulse_width_us = self
            .pulse_width_us
            .ok_or(ControlError::MissingPulse)?;

        self.backend
            .start(pulse_width_us)
            .map_err(|error| self.record_error(error))
    }

    pub fn service(&mut self) {
        self.backend.service();

        if self.backend.is_faulted() {
            self.flags |= OUTPUT_ERROR;
        }
    }

    fn check_output_fault(&mut self) -> Result<(), ControlError> {
        if self.backend.is_faulted() {
            self.flags |= OUTPUT_ERROR;
        }

        if self.flags & OUTPUT_ERROR != 0 {
            return Err(ControlError::OutputError);
        }

        Ok(())
    }

    fn record_error(&mut self, error: BackendError) -> ControlError {
        match error {
            BackendError::Busy => ControlError::Busy,
            BackendError::OutputError => {
                self.flags |= OUTPUT_ERROR;
                ControlError::OutputError
            }
        }
    }
}

#[cfg(test)]
#[path = "test_control.rs"]
mod tests;
