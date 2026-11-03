use crate::{ControlRegister, OutputBackend};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_protocol::{
    Access, Frame, Header, Opcode, ParsedFrame, RegisterRegion,
    ValidationError, validate_header,
};

/// Fixed registers and the current eighteen-servo target region.
pub const REGISTER_REGIONS: [RegisterRegion; 5] = [
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

pub struct RegisterService<O: OutputBackend> {
    protocol_version: u16,
    servo_count: u8,
    control: ControlRegister<O>,
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
                1 => (1, self.control.status(), 4),
                3 => (3, u32::from(self.control.value()), 1),
                4 => (4, u32::from(self.servo_count), 1),

                // Successful validation against REGISTER_REGIONS should
                // make this unreachable. Keep failure local and explicit.
                _ => return Err(ServiceError::InvalidMap),
            }
        };

        // Do not change LastError or the output buffer if the caller
        // failed to provide enough space for the response.
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

    /// Services output independently of incoming register requests.
    ///
    /// The application loop must call this regularly.
    pub fn service(&mut self) {
        self.control.service();
    }
}

#[cfg(test)]
#[path = "test_service.rs"]
mod tests;
