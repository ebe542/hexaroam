//! Hardware-independent GPIO signal plans.

use crate::PulseLimits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseTarget {
    pub gpio: u8,
    pub pulse_width_us: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalPlanEntry {
    pub gpio_mask: u32,
    pub duration_us: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalPlanError {
    EmptyTargets,
    InvalidPeriod,
    InvalidGpio(u8),
    DuplicateGpio(u8),
    InvalidPulseWidth(u8),
    InsufficientCapacity,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SignalPlan<const CAPACITY: usize> {
    entries: [SignalPlanEntry; CAPACITY],
    len: usize,
    period_us: u32,
}

impl<const CAPACITY: usize> SignalPlan<CAPACITY> {
    pub fn new(
        targets: &[PulseTarget],
        limits: &PulseLimits,
        period_us: u32,
    ) -> Result<Self, SignalPlanError> {
        if period_us == 0 {
            return Err(SignalPlanError::InvalidPeriod);
        }

        if targets.is_empty() {
            return Err(SignalPlanError::EmptyTargets);
        }

        let mut gpio_mask = 0_u32;

        // Validate all targets before constructing the plan.
        for target in targets {
            if target.gpio >= 32 {
                return Err(SignalPlanError::InvalidGpio(
                    target.gpio,
                ));
            }

            let gpio_bit = 1_u32 << target.gpio;

            if gpio_mask & gpio_bit != 0 {
                return Err(SignalPlanError::DuplicateGpio(
                    target.gpio,
                ));
            }

            if target.pulse_width_us == 0
                || target.pulse_width_us >= period_us
                || !limits.contains(target.pulse_width_us)
            {
                return Err(SignalPlanError::InvalidPulseWidth(
                    target.gpio,
                ));
            }

            gpio_mask |= gpio_bit;
        }

        let mut plan = Self {
            entries: [SignalPlanEntry {
                gpio_mask: 0,
                duration_us: 0,
            }; CAPACITY],
            len: 0,
            period_us,
        };

        let mut elapsed_us = 0;

        // Visit distinct pulse end times in ascending order.
        while gpio_mask != 0 {
            let next_end_us = targets
                .iter()
                .map(|target| target.pulse_width_us)
                .filter(|&end_us| end_us > elapsed_us)
                .min()
                .expect("An active GPIO must have a future pulse end");

            if plan.len == CAPACITY {
                return Err(SignalPlanError::InsufficientCapacity);
            }

            plan.entries[plan.len] = SignalPlanEntry {
                gpio_mask,
                duration_us: next_end_us - elapsed_us,
            };
            plan.len += 1;

            // Outputs with equal end times change together.
            for target in targets {
                if target.pulse_width_us == next_end_us {
                    gpio_mask &= !(1_u32 << target.gpio);
                }
            }

            elapsed_us = next_end_us;
        }

        // Complete the period with all outputs Low.
        if plan.len == CAPACITY {
            return Err(SignalPlanError::InsufficientCapacity);
        }

        plan.entries[plan.len] = SignalPlanEntry {
            gpio_mask: 0,
            duration_us: period_us - elapsed_us,
        };
        plan.len += 1;

        Ok(plan)
    }

    pub fn entries(&self) -> &[SignalPlanEntry] {
        &self.entries[..self.len]
    }

    pub fn period_us(&self) -> u32 {
        self.period_us
    }
}

#[cfg(test)]
#[path = "test_signal_plan.rs"]
mod tests;
