#![no_std]

//! Hardware-independent servo angles and calibration.

mod angle;

pub use angle::{Angle, AngleError};

mod calibration;

pub use calibration::{Calibration, CalibrationError};
