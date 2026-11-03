use crate::{ControlRegister, OutputBackend};
use crate::{PendingCalibration, STATUS_OUTPUT_ACTIVE};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_protocol::{
    Access, Frame, Header, Opcode, ParsedFrame, RegisterRegion,
    ValidationError, validate_header,
};
use hexaroam_servo_storage::SoftwareCrc32;

const CALIBRATION_INDEX: u8 = 0x10;

// Serialized format: 4-byte header + 18 entries of 20 bytes + CRC32.
const CALIBRATION_BYTES: u16 = 4 + 18 * 20 + 4;

const STATUS_BUSY: u32 = 1 << 2;
const RESULT_SHIFT: u32 = 8;

/// Fixed registers and the current eighteen-servo target region.
pub const REGISTER_REGIONS: [RegisterRegion; 6] = [
    RegisterRegion {
        first_index: 0,
        entries: 1,
        bytes_per_entry: 2,
        access: Access::ReadOnly,
    },
    RegisterRegion {
        first_index: 1,
        entries: 1,
        bytes_per_entry: 4,
        access: Access::ReadOnly,
    },
    RegisterRegion {
        first_index: 3,
        entries: 1,
        bytes_per_entry: 1,
        access: Access::ReadWriteBits,
    },
    RegisterRegion {
        first_index: 4,
        entries: 1,
        bytes_per_entry: 1,
        access: Access::ReadOnly,
    },
    RegisterRegion {
        first_index: CALIBRATION_INDEX,
        entries: 1,
        bytes_per_entry: CALIBRATION_BYTES,
        access: Access::ReadWrite,
    },
    RegisterRegion {
        first_index: 0x20,
        entries: 18,
        bytes_per_entry: 4,
        access: Access::ReadWrite,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCode {
    None = 0,
    UnknownIndex = 1,
    UnsupportedAccess = 2,
    InvalidLength = 3,
    OutOfBounds = 4,
    InvalidValue = 5,
    NotConfigured = 6,
    InvalidState = 7,
    OutputError = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceError {
    ResponseBufferTooSmall,
    InvalidMap,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CalibrationSaveResult {
    Success = 1,
    StorageError = 2,
    VerificationFailed = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationCompletionError {
    NoPendingCalibration,
}

pub struct RegisterService<O: OutputBackend> {
    protocol_version: u16,
    servo_count: u8,
    control: ControlRegister<O>,
    pending_calibration: Option<PendingCalibration<18>>,
    calibration_result: Option<CalibrationSaveResult>,
}

impl<O: OutputBackend> RegisterService<O> {
    pub fn new(
        protocol_version: u16,
        servo_count: u8,
        output: O,
        calibration_valid: bool,
    ) -> Self {
        Self {
            protocol_version,
            servo_count,
            control: ControlRegister::new(output, calibration_valid),
            pending_calibration: None,
            calibration_result: None,
        }
    }

    /// Handles a CRC-checked request.
    ///
    /// Every handled request returns an ACK or NACK referencing output.
    /// Successful reads return data; successful modifications return no payload.
    /// Rejected requests return one error byte and the original register index.
    ///
    /// Local response-buffer errors leave registers and output unchanged.
    pub fn handle<'out>(
        &mut self,
        request: ParsedFrame<'_>,
        output: &'out mut [u8],
    ) -> Result<Option<Frame<'out>>, ServiceError> {
        let header = request.frame.header;
        let payload = request.frame.payload;

        // Preserve the caller's buffer if response capacity is insufficient.
        let validation = request
            .validation
            .and_then(|()| validate_header(&header, &REGISTER_REGIONS));

        let mut failure = match validation {
            Ok(()) => None,
            Err(error) => Some(match error {
                ValidationError::InvalidMap => {
                    return Err(ServiceError::InvalidMap);
                }
                ValidationError::UnknownIndex => ErrorCode::UnknownIndex,
                ValidationError::UnsupportedAccess => {
                    ErrorCode::UnsupportedAccess
                }
                ValidationError::InvalidLength => ErrorCode::InvalidLength,
                ValidationError::OutOfBounds => ErrorCode::OutOfBounds,
            }),
        };

        if failure.is_none() {
            let expected_payload_length = if header.opcode == Opcode::Read {
                0
            } else {
                usize::from(header.length)
            };

            if payload.len() != expected_payload_length {
                failure = Some(ErrorCode::InvalidLength);
            }
        }

        // Pending ownership is the source of truth for Busy. Reject changes
        // before target-count checks or backend calls can have any effect.
        if failure.is_none()
            && header.opcode != Opcode::Read
            && self.pending_calibration.is_some()
        {
            failure = Some(ErrorCode::InvalidState);
        }

        // The map permits READ for structural plausibility checks, but
        // calibration readback is not part of this implementation step.
        if failure.is_none()
            && header.opcode == Opcode::Read
            && header.index == CALIBRATION_INDEX
        {
            failure = Some(ErrorCode::UnsupportedAccess);
        }

        if failure.is_none() && (0x20..=0x31).contains(&header.index) {
            let first = usize::from(header.index - 0x20);
            let count = usize::from(header.length) / 4;

            // Header validation already guarantees a nonempty,
            // four-byte-aligned range within the target region.
            if first + count > usize::from(self.servo_count) {
                failure = Some(ErrorCode::OutOfBounds);
            } else if header.opcode == Opcode::Read {
                // Stage the complete response before touching the caller's
                // buffer. A later read failure must not expose partial data.
                let mut bytes = [0_u8; 18 * 4];

                for offset in 0..count {
                    match self
                        .control
                        .target(ServoId(first + offset))
                    {
                        Ok(angle) => {
                            let start = offset * 4;
                            bytes[start..start + 4].copy_from_slice(
                                &angle.as_centidegrees().to_le_bytes(),
                            );
                        }
                        Err(error) => {
                            failure = Some(error);
                            break;
                        }
                    }
                }

                if failure.is_none() {
                    let length = usize::from(header.length);

                    if output.len() < length {
                        return Err(ServiceError::ResponseBufferTooSmall);
                    }

                    output[..length].copy_from_slice(&bytes[..length]);

                    return Ok(Some(Frame {
                        header: Header {
                            opcode: Opcode::Ack,
                            index: header.index,
                            length: header.length,
                        },
                        payload: &output[..length],
                    }));
                }

                // Return a NACK through the common read-response path below.
            } else {
                // Access validation permits only WRITE for target registers.
                let mut targets = [ServoTarget {
                    id: ServoId(0),
                    angle: Angle::from_centidegrees(0),
                }; 18];

                for (offset, target) in targets[..count].iter_mut().enumerate()
                {
                    let start = offset * 4;
                    let centidegrees = i32::from_le_bytes([
                        payload[start],
                        payload[start + 1],
                        payload[start + 2],
                        payload[start + 3],
                    ]);

                    *target = ServoTarget {
                        id: ServoId(first + offset),
                        angle: Angle::from_centidegrees(centidegrees),
                    };
                }

                // Reserve space for a possible NACK before changing output.
                if output.is_empty() {
                    return Err(ServiceError::ResponseBufferTooSmall);
                }

                let result = self
                    .control
                    .set_targets(&targets[..count]);

                return match result {
                    Ok(()) => Ok(Some(Frame {
                        header: Header {
                            opcode: Opcode::Ack,
                            index: header.index,
                            length: 0,
                        },
                        payload: &output[..0],
                    })),
                    Err(error) => {
                        output[0] = error as u8;

                        Ok(Some(Frame {
                            header: Header {
                                opcode: Opcode::Nack,
                                index: header.index,
                                length: 1,
                            },
                            payload: &output[..1],
                        }))
                    }
                };
            }
        }

        if header.opcode != Opcode::Read {
            // A failed modifying request needs one byte for its error response.
            // Check capacity before executing any operation.
            if output.is_empty() {
                return Err(ServiceError::ResponseBufferTooSmall);
            }

            let result = if let Some(error) = failure {
                Err(error)
            } else if header.index == CALIBRATION_INDEX {
                if header.opcode != Opcode::Write {
                    Err(ErrorCode::UnsupportedAccess)
                } else if self.control.status() & STATUS_OUTPUT_ACTIVE != 0 {
                    Err(ErrorCode::InvalidState)
                } else {
                    match PendingCalibration::<18>::from_bytes(
                        payload,
                        &mut SoftwareCrc32,
                    ) {
                        Ok(pending) => {
                            // Publish the complete validated request only.
                            // Active calibration and ServoCount stay unchanged.
                            self.calibration_result = None;
                            self.pending_calibration = Some(pending);
                            Ok(())
                        }
                        Err(_) => Err(ErrorCode::InvalidValue),
                    }
                }
            } else if header.index == 3 {
                self.control
                    .apply(header.opcode, payload[0])
            } else {
                Err(ErrorCode::UnsupportedAccess)
            };

            return match result {
                Ok(()) => Ok(Some(Frame {
                    header: Header {
                        opcode: Opcode::Ack,
                        index: header.index,
                        length: 0,
                    },
                    payload: &output[..0],
                })),
                Err(error) => {
                    output[0] = error as u8;

                    Ok(Some(Frame {
                        header: Header {
                            opcode: Opcode::Nack,
                            index: header.index,
                            length: 1,
                        },
                        payload: &output[..1],
                    }))
                }
            };
        }

        let (response_index, value, width) = if let Some(error) = failure {
            // Report the error against the original requested register.
            (header.index, error as u32, 1_usize)
        } else {
            match header.index {
                0 => (0, u32::from(self.protocol_version), 2),
                1 => (1, self.status(), 4),
                3 => (3, u32::from(self.control.value()), 1),
                4 => (4, u32::from(self.servo_count), 1),

                // Successful validation against REGISTER_REGIONS should
                // make this unreachable. Keep failure local and explicit.
                _ => return Err(ServiceError::InvalidMap),
            }
        };

        // Preserve the caller's buffer if response capacity is insufficient.
        if output.len() < width {
            return Err(ServiceError::ResponseBufferTooSmall);
        }

        let bytes = value.to_le_bytes();
        output[..width].copy_from_slice(&bytes[..width]);

        Ok(Some(Frame {
            header: Header {
                opcode: if failure.is_some() {
                    Opcode::Nack
                } else {
                    Opcode::Ack
                },
                index: response_index,
                length: width as u16,
            },
            payload: &output[..width],
        }))
    }

    fn status(&self) -> u32 {
        let busy = if self.pending_calibration.is_some() {
            STATUS_BUSY
        } else {
            0
        };
        let result = self
            .calibration_result
            .map_or(0, |result| {
                (result as u32) << RESULT_SHIFT
            });

        self.control.status() | busy | result
    }

    /// Services output independently of incoming register requests.
    ///
    /// The application loop must call this regularly.
    pub fn service(&mut self) {
        self.control.service();
    }

    /// Borrows the accepted request without clearing Busy.
    ///
    /// Presence does not imply that its ACK has finished transmission.
    pub fn pending_calibration(&self) -> Option<&PendingCalibration<18>> {
        self.pending_calibration.as_ref()
    }

    /// Records completion, releases the request and clears Busy.
    ///
    /// This does not replace the active calibration.
    pub fn complete_calibration(
        &mut self,
        result: CalibrationSaveResult,
    ) -> Result<(), CalibrationCompletionError> {
        if self.pending_calibration.is_none() {
            return Err(CalibrationCompletionError::NoPendingCalibration);
        }

        // Publish the result before releasing the request and clearing Busy.
        self.calibration_result = Some(result);
        self.pending_calibration = None;

        Ok(())
    }
}

#[cfg(test)]
#[path = "test_service.rs"]
mod tests;
