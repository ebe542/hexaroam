//! Calibrated angle commands for a pulse output.

use crate::{Angle, Calibration, CalibrationError};

pub trait PulseOutput {
    type Error;

    fn set_pulse_width_us(
        &mut self,
        pulse_width_us: u32,
    ) -> Result<(), Self::Error>;
}

pub trait ServoChannel {
    type Error;

    fn validate_angle(
        &self,
        target_angle: Angle,
    ) -> Result<(), CalibrationError>;

    fn set_angle(
        &mut self,
        target_angle: Angle,
    ) -> Result<(), ServoError<Self::Error>>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServoError<E> {
    Calibration(CalibrationError),
    Output(E),
}

pub struct Servo<O: PulseOutput> {
    calibration: Calibration,
    output: O,
}

impl<O: PulseOutput> Servo<O> {
    pub fn new(calibration: Calibration, output: O) -> Self {
        Self {
            calibration,
            output,
        }
    }

    pub fn set_angle(
        &mut self,
        target_angle: Angle,
    ) -> Result<(), ServoError<O::Error>> {
        let pulse_width_us = self
            .calibration
            .pulse_width_us(target_angle)
            .map_err(ServoError::Calibration)?;

        self.output
            .set_pulse_width_us(pulse_width_us)
            .map_err(ServoError::Output)
    }

    pub fn validate_angle(
        &self,
        target_angle: Angle,
    ) -> Result<(), CalibrationError> {
        self.calibration
            .pulse_width_us(target_angle)
            .map(|_| ())
    }
}

impl<O: PulseOutput> ServoChannel for Servo<O> {
    type Error = O::Error;

    fn validate_angle(
        &self,
        target_angle: Angle,
    ) -> Result<(), CalibrationError> {
        Servo::validate_angle(self, target_angle)
    }

    fn set_angle(
        &mut self,
        target_angle: Angle,
    ) -> Result<(), ServoError<Self::Error>> {
        Servo::set_angle(self, target_angle)
    }
}

#[cfg(test)]
#[path = "test_servo.rs"]
mod tests;
