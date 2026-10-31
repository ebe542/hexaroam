//! Control-register transitions independent of protocol dispatch.

use hexaroam_servo_protocol::Opcode;

use crate::{ErrorCode, OutputBackend, OutputStartError, OutputState};

pub const OUTPUT_ENABLE: u8 = 1 << 0;

pub const STATUS_CALIBRATION_VALID: u8 = 1 << 0;
pub const STATUS_OUTPUT_ACTIVE: u8 = 1 << 1;
pub const STATUS_FAULT: u8 = 1 << 3;

pub struct ControlRegister<O: OutputBackend> {
    output: O,
    calibration_valid: bool,
    enable_requested: bool,
    faulted: bool,
}

impl<O: OutputBackend> ControlRegister<O> {
    pub fn new(output: O, calibration_valid: bool) -> Self {
        let enable_requested = output.state() == OutputState::Active;

        Self {
            output,
            calibration_valid,
            enable_requested,
            faulted: false,
        }
    }

    /// Returns the last successfully accepted enable request.
    pub fn value(&self) -> u8 {
        if self.enable_requested {
            OUTPUT_ENABLE
        } else {
            0
        }
    }

    /// Reports calibration, actual backend state and latched fault.
    pub fn status(&self) -> u8 {
        let mut status = 0;

        if self.calibration_valid {
            status |= STATUS_CALIBRATION_VALID;
        }

        if self.output.state() == OutputState::Active {
            status |= STATUS_OUTPUT_ACTIVE;
        }

        if self.faulted {
            status |= STATUS_FAULT;
        }

        status
    }

    /// Applies WRITE, SET or CLR after validating the request.
    ///
    /// Control changes only after a successful backend operation.
    /// Technical failures latch Fault; invalid targets do not.
    /// Disabling output remains possible in Fault.
    pub fn apply(
        &mut self,
        opcode: Opcode,
        value: u8,
    ) -> Result<(), ErrorCode> {
        let candidate = match opcode {
            Opcode::Read => {
                return Err(ErrorCode::UnsupportedAccess);
            }
            Opcode::Write => value,
            Opcode::Set => self.value() | value,
            Opcode::Clear => self.value() & !value,
        };

        // Reject reserved bits even when clearing them would otherwise
        // leave the resulting control value unchanged.
        if value & !OUTPUT_ENABLE != 0 {
            return Err(ErrorCode::InvalidValue);
        }

        let enable = candidate & OUTPUT_ENABLE != 0;

        if enable {
            if self.faulted {
                return Err(ErrorCode::InvalidState);
            }

            if !self.calibration_valid {
                return Err(ErrorCode::NotConfigured);
            }

            if self.output.state() != OutputState::Active {
                match self.output.start() {
                    Ok(()) => {}
                    Err(OutputStartError::InvalidTargets) => {
                        // The backend contract guarantees unchanged output.
                        return Err(ErrorCode::InvalidValue);
                    }
                    Err(OutputStartError::Backend(_)) => {
                        self.faulted = true;
                        return Err(ErrorCode::OutputError);
                    }
                }
            }
        } else if self.output.state() == OutputState::Active {
            if self.output.stop().is_err() {
                self.faulted = true;
                return Err(ErrorCode::OutputError);
            }
        }

        // Commit only after success. A successful stop does not clear
        // an existing fault; status() always reads actual backend state.
        self.enable_requested = enable;

        Ok(())
    }
}

#[cfg(test)]
#[path = "test_control.rs"]
mod tests;
