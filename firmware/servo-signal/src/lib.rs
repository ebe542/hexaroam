#![no_std]

//! Pulse validation and transport for the HexaRoam servo signal protocol.

#[derive(Debug, PartialEq, Eq)]
pub enum PulseError {
    OutOfRange,
    Busy,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LimitsError {
    MinExceedsMax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PulseLimits {
    min_us: u32,
    max_us: u32,
}

impl PulseLimits {
    pub fn new(min_us: u32, max_us: u32) -> Result<Self, LimitsError> {
        if min_us > max_us {
            return Err(LimitsError::MinExceedsMax);
        }

        Ok(Self { min_us, max_us })
    }

    pub fn contains(&self, pulse_width_us: u32) -> bool {
        (self.min_us..=self.max_us).contains(&pulse_width_us)
    }

    pub fn min_us(&self) -> u32 {
        self.min_us
    }

    pub fn max_us(&self) -> u32 {
        self.max_us
    }
}

pub trait PulseTransport {
    fn is_empty(&self) -> bool;

    fn write(&mut self, pulse_width_us: u32) -> bool;
}

pub fn submit_pulse(
    transport: &mut impl PulseTransport,
    limits: &PulseLimits,
    pulse_width_us: u32,
) -> Result<(), PulseError> {
    if !limits.contains(pulse_width_us) {
        return Err(PulseError::OutOfRange);
    }

    if !transport.is_empty() {
        return Err(PulseError::Busy);
    }

    if !transport.write(pulse_width_us) {
        return Err(PulseError::Busy);
    }

    Ok(())
}

#[cfg(test)]
mod tests;
