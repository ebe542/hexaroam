use crate::{ControlRegister, OutputBackend};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_protocol::{
    Access, Frame, Header, Opcode, ParsedFrame, RegisterRegion,
    ValidationError, validate_header,
};

/// Initial subset; Control and variable-length regions follow separately.
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
        bytes_per_entry: 1,
        access: Access::ReadOnly,
    },
    RegisterRegion {
        first_index: 2,
        entries: 1,
        bytes_per_entry: 4,
        access: Access::ReadWrite,
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
#[repr(u32)]
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
    last_error: ErrorCode,
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
            last_error: ErrorCode::None,
        }
    }

    /// Handles a CRC-checked request.
    ///
    /// READ returns a response referencing output.
    /// WRITE/SET/CLR return None, including rejected requests.
    ///
    /// Failed requests update LastError; successful accesses preserve it
    /// except an explicit four-byte WRITE of zero to LastError.
    ///
    /// Local response-buffer errors leave registers and output unchanged.
    pub fn handle<'out>(
        &mut self,
        request: ParsedFrame<'_>,
        output: &'out mut [u8],
    ) -> Result<Option<Frame<'out>>, ServiceError> {
        let header = request.frame.header;
        let payload = request.frame.payload;

        // Preserve a parser-reported rejection. Otherwise validate against
        // this service's own map, protecting its public calling interface.
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
                            opcode: Opcode::Read,
                            index: header.index,
                            length: header.length,
                        },
                        payload: &output[..length],
                    }));
                }

                // Use the existing canonical LastError response below.
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

                // Submit once, after decoding the complete request.
                // Calibration validation and atomic acceptance belong
                // to the backend, not the register service.
                if let Err(error) = self
                    .control
                    .set_targets(&targets[..count])
                {
                    self.last_error = error;
                }

                // Successful writes preserve any previous LastError.
                // Target writes never produce a direct response.
                return Ok(None);
            }
        }

        if header.opcode != Opcode::Read {
            if let Some(error) = failure {
                self.last_error = error;
                return Ok(None);
            }

            if header.index == 3 {
                // Header and payload validation above guarantee exactly one byte.
                if let Err(error) = self
                    .control
                    .apply(header.opcode, payload[0])
                {
                    self.last_error = error;
                }

                // Successful control operations preserve any previous LastError.
                // WRITE, SET and CLR never produce a direct response.
                return Ok(None);
            }

            // Control and target writes were handled above.
            // The remaining writable register is LastError.
            if header.opcode != Opcode::Write || header.index != 2 {
                self.last_error = ErrorCode::UnsupportedAccess;
                return Ok(None);
            }

            // Header validation and payload-length validation establish
            // exactly four bytes before these indexed reads.
            let value = u32::from_le_bytes([
                payload[0], payload[1], payload[2], payload[3],
            ]);

            self.last_error = if value == 0 {
                ErrorCode::None
            } else {
                ErrorCode::InvalidValue
            };

            return Ok(None);
        }

        let (response_index, value, width) = if let Some(error) = failure {
            // Canonical error response: READ LastError, four bytes.
            (2, error as u32, 4_usize)
        } else {
            match header.index {
                0 => (0, u32::from(self.protocol_version), 2),
                1 => (1, u32::from(self.control.status()), 1),
                2 => (2, self.last_error as u32, 4),
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

        if let Some(error) = failure {
            self.last_error = error;
        }

        Ok(Some(Frame {
            header: Header {
                opcode: Opcode::Read,
                index: response_index,
                length: width as u16,
            },
            payload: &output[..width],
        }))
    }
}

#[cfg(test)]
#[path = "test_service.rs"]
mod tests;
