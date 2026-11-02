#![no_std]

//! Register access and command handling for the HexaRoam servo controller.

mod service;

pub use service::{ErrorCode, REGISTER_REGIONS, RegisterService, ServiceError};

mod output;

pub use output::{
    OutputBackend, OutputStartError, OutputState, TargetReadError,
    TargetWriteError,
};

mod control;

pub use control::{
    ControlRegister, OUTPUT_ENABLE, STATUS_CALIBRATION_VALID, STATUS_FAULT,
    STATUS_OUTPUT_ACTIVE,
};

mod servo_bank_backend;

pub use servo_bank_backend::ServoBankBackendError;

mod frame_processor;

pub use frame_processor::{
    FrameProcessingError, FrameProcessor, FrameProgress,
};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod test_pipeline;

#[cfg(test)]
mod test_targets;

#[cfg(test)]
mod test_servo_bank_backend;
