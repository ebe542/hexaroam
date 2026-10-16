#![no_std]

//! Hardware-independent servo angles and calibration.

mod angle;

pub use angle::{Angle, AngleError};

mod calibration;

pub use calibration::{Calibration, CalibrationError};

mod servo;

pub use servo::{PulseOutput, Servo, ServoChannel, ServoError};

mod servo_bank;

pub use servo_bank::{ServoBank, ServoBankError, ServoId, ServoTarget};
