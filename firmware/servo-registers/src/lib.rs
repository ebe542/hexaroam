#![no_std]

//! Register access and command handling for the HexaRoam servo controller.

mod service;

pub use service::{ErrorCode, REGISTER_REGIONS, RegisterService, ServiceError};

mod output;

pub use output::{OutputBackend, OutputStartError, OutputState};

mod control;

pub use control::{
    ControlRegister, OUTPUT_ENABLE, STATUS_CALIBRATION_VALID, STATUS_FAULT,
    STATUS_OUTPUT_ACTIVE,
};

#[cfg(test)]
mod test_support;
