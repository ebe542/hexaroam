#![no_std]

//! Register access and command handling for the HexaRoam servo controller.

mod service;

pub use service::{
    CalibrationCompletionError, CalibrationSaveResult, ErrorCode,
    REGISTER_REGIONS, RegisterService, ServiceError,
};

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

mod pending_calibration;

pub use pending_calibration::{PendingCalibration, PendingCalibrationError};

mod calibration_persistence;

pub use calibration_persistence::{
    CalibrationPersistenceError, persist_calibration,
};

pub mod command_service;

pub mod command_processor;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod test_pipeline;

#[cfg(test)]
mod test_targets;

#[cfg(test)]
mod test_servo_bank_backend;

#[cfg(test)]
mod test_calibration_request;
