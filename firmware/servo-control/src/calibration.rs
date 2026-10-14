//! Calibration between joint angles and servo pulse widths.

use crate::Angle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationError {
    InvalidAngleRange,
    ZeroPulseWidth,
    EqualPulseWidths,
    AngleOutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    lower_angle: Angle,
    lower_pulse_us: u32,
    upper_angle: Angle,
    upper_pulse_us: u32,
}

impl Calibration {
    pub fn new(
        lower_angle: Angle,
        lower_pulse_us: u32,
        upper_angle: Angle,
        upper_pulse_us: u32,
    ) -> Result<Self, CalibrationError> {
        if lower_angle >= upper_angle {
            return Err(CalibrationError::InvalidAngleRange);
        }

        if lower_pulse_us == 0 || upper_pulse_us == 0 {
            return Err(CalibrationError::ZeroPulseWidth);
        }

        if lower_pulse_us == upper_pulse_us {
            return Err(CalibrationError::EqualPulseWidths);
        }

        Ok(Self {
            lower_angle,
            lower_pulse_us,
            upper_angle,
            upper_pulse_us,
        })
    }

    pub fn pulse_width_us(
        &self,
        target_angle: Angle,
    ) -> Result<u32, CalibrationError> {
        if target_angle < self.lower_angle || target_angle > self.upper_angle {
            return Err(CalibrationError::AngleOutOfRange);
        }

        let lower = i64::from(self.lower_angle.as_centidegrees());
        let upper = i64::from(self.upper_angle.as_centidegrees());
        let target = i64::from(target_angle.as_centidegrees());

        let span = (upper - lower) as u64;
        let offset = (target - lower) as u64;

        let weighted_sum = u64::from(self.lower_pulse_us) * (span - offset)
            + u64::from(self.upper_pulse_us) * offset;

        // Round to the nearest microsecond; ties round upward.
        let pulse_width_us = (weighted_sum + span / 2) / span;

        Ok(pulse_width_us as u32)
    }
}

#[cfg(test)]
#[path = "test_calibration.rs"]
mod tests;
