//! Converts complete servo pulse batches into GPIO signal plans.

use hexaroam_servo_signal::{
    PulseLimits, PulseTarget, SignalOutputState, SignalPlan, SignalPlanError,
    SignalPlanLifecycle, SignalPlanOutput,
};

use crate::{PulseBankLifecycle, PulseBankOutput};

#[derive(Debug, PartialEq, Eq)]
pub enum ServoSignalOutputInitError {
    EmptyOutputs,
    InvalidGpio(u8),
    DuplicateGpio(u8),
    InvalidPeriod,
    InsufficientCapacity,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServoSignalOutputError<E> {
    PulseCountMismatch { expected: usize, actual: usize },
    SignalPlan(SignalPlanError),
    Output(E),
}

pub struct ServoSignalOutput<
    O: SignalPlanOutput,
    const N: usize,
    const PLAN_CAPACITY: usize,
> {
    output: O,
    gpio_assignments: [u8; N],
    limits: PulseLimits,
    period_us: u32,
}

impl<O: SignalPlanOutput, const N: usize, const PLAN_CAPACITY: usize>
    ServoSignalOutput<O, N, PLAN_CAPACITY>
{
    /// Validates configuration without submitting a plan.
    ///
    /// GPIOs must be unique and within the generic range 0..32.
    /// The period must be nonzero.
    /// Capacity must accommodate the worst case of N + 1 entries.
    pub fn new(
        output: O,
        gpio_assignments: [u8; N],
        limits: PulseLimits,
        period_us: u32,
    ) -> Result<Self, ServoSignalOutputInitError> {
        if N == 0 {
            return Err(ServoSignalOutputInitError::EmptyOutputs);
        }

        for (index, &gpio) in gpio_assignments.iter().enumerate() {
            if gpio >= 32 {
                return Err(ServoSignalOutputInitError::InvalidGpio(
                    gpio,
                ));
            }

            if gpio_assignments[..index].contains(&gpio) {
                return Err(ServoSignalOutputInitError::DuplicateGpio(gpio));
            }
        }

        if period_us == 0 {
            return Err(ServoSignalOutputInitError::InvalidPeriod);
        }

        // N distinct pulse endings plus the final all-Low interval.
        // Comparing directly avoids calculating N + 1.
        if PLAN_CAPACITY <= N {
            return Err(ServoSignalOutputInitError::InsufficientCapacity);
        }

        Ok(Self {
            output,
            gpio_assignments,
            limits,
            period_us,
        })
    }

    /// Builds a complete plan without changing backend state.
    ///
    /// The generic error parameter lets submission and lifecycle calls
    /// share validation while retaining their distinct backend errors.
    fn build_plan<E>(
        &self,
        pulse_widths_us: &[u32],
    ) -> Result<SignalPlan<PLAN_CAPACITY>, ServoSignalOutputError<E>> {
        if pulse_widths_us.len() != N {
            return Err(
                ServoSignalOutputError::PulseCountMismatch {
                    expected: N,
                    actual: pulse_widths_us.len(),
                },
            );
        }

        let targets: [PulseTarget; N] =
            core::array::from_fn(|index| PulseTarget {
                gpio: self.gpio_assignments[index],
                pulse_width_us: pulse_widths_us[index],
            });

        SignalPlan::<PLAN_CAPACITY>::new(&targets, &self.limits, self.period_us)
            .map_err(ServoSignalOutputError::SignalPlan)
    }
}

impl<O: SignalPlanOutput, const N: usize, const PLAN_CAPACITY: usize>
    PulseBankOutput for ServoSignalOutput<O, N, PLAN_CAPACITY>
{
    type Error = ServoSignalOutputError<O::Error>;

    fn is_busy_error(error: &Self::Error) -> bool {
        match error {
            ServoSignalOutputError::Output(error) => O::is_busy_error(error),
            _ => false,
        }
    }

    fn submit_pulses(
        &mut self,
        pulse_widths_us: &[u32],
    ) -> Result<(), Self::Error> {
        let plan = self.build_plan::<O::Error>(pulse_widths_us)?;

        self.output
            .submit_plan(&plan)
            .map_err(ServoSignalOutputError::Output)
    }

    fn service(&mut self) {
        self.output.service();
    }
}

impl<O: SignalPlanLifecycle, const N: usize, const PLAN_CAPACITY: usize>
    ServoSignalOutput<O, N, PLAN_CAPACITY>
{
    /// Builds and validates the complete initial plan before startup.
    ///
    /// Pulses follow ServoId order and use the configured GPIO mapping.
    /// Validation errors must not call the lifecycle backend.
    pub fn start_pulses(
        &mut self,
        pulse_widths_us: &[u32],
    ) -> Result<(), ServoSignalOutputError<O::LifecycleError>> {
        let plan = self.build_plan::<O::LifecycleError>(pulse_widths_us)?;

        self.output
            .start_plan(&plan)
            .map_err(ServoSignalOutputError::Output)
    }

    /// Stops output through the lifecycle backend without retry.
    pub fn stop(&mut self) -> Result<(), O::LifecycleError> {
        self.output.stop()
    }

    /// Returns the backend's confirmed logical output state.
    pub fn state(&self) -> SignalOutputState {
        self.output.state()
    }
}

impl<O: SignalPlanLifecycle, const N: usize, const PLAN_CAPACITY: usize>
    PulseBankLifecycle for ServoSignalOutput<O, N, PLAN_CAPACITY>
{
    type LifecycleError = ServoSignalOutputError<O::LifecycleError>;

    fn start_pulses(
        &mut self,
        pulse_widths_us: &[u32],
    ) -> Result<(), Self::LifecycleError> {
        ServoSignalOutput::start_pulses(self, pulse_widths_us)
    }

    fn stop(&mut self) -> Result<(), Self::LifecycleError> {
        ServoSignalOutput::stop(self).map_err(ServoSignalOutputError::Output)
    }

    fn state(&self) -> SignalOutputState {
        ServoSignalOutput::state(self)
    }
}

#[cfg(test)]
#[path = "test_servo_signal_output.rs"]
mod tests;
