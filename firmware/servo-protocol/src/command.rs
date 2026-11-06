//! Request decoding after COBS and CRC validation.
//!
//! This module checks wire structure only. Servo configuration,
//! calibration limits and controller state belong to the service layer.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    GetStatus = 0x01,
    OutputEnable = 0x02,
    SetTargets = 0x03,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NackCode {
    UnknownCommand = 0x01,
    InvalidLength = 0x02,
    InvalidValue = 0x03,
    InvalidState = 0x04,
    Busy = 0x05,
    OutputError = 0x06,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    // No command byte exists, so no NACK can be constructed.
    EmptyInput,
    Rejected { command: u8, code: NackCode },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetBatch<'a> {
    pub first_servo: u8,
    angle_bytes: &'a [u8],
}

impl TargetBatch<'_> {
    pub fn angle_count(&self) -> usize {
        self.angle_bytes.len() / 4
    }

    /// Returns a signed little-endian centidegree value.
    pub fn angle(&self, index: usize) -> Option<i32> {
        let start = index.checked_mul(4)?;
        let end = start.checked_add(4)?;
        let bytes = self.angle_bytes.get(start..end)?;

        Some(i32::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
        ]))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request<'a> {
    GetStatus,
    OutputEnable(bool),
    SetTargets(TargetBatch<'a>),
}

/// Decodes content including its command byte, excluding CRC.
///
/// Ok(None) means an incoming ACK/NACK was ignored.
/// Unknown commands and malformed request parameters are rejected.
/// SET_TARGETS accepts 1..=62 angles; physical limits are checked later.
pub fn decode(content: &[u8]) -> Result<Option<Request<'_>>, CommandError> {
    let (&command, parameters) = content
        .split_first()
        .ok_or(CommandError::EmptyInput)?;

    // Response codes are never interpreted as requests.
    if matches!(command, 0x80 | 0x81) {
        return Ok(None);
    }

    let reject = |code| CommandError::Rejected { command, code };

    match command {
        value if value == Command::GetStatus as u8 => {
            if !parameters.is_empty() {
                return Err(reject(NackCode::InvalidLength));
            }

            Ok(Some(Request::GetStatus))
        }

        value if value == Command::OutputEnable as u8 => {
            if parameters.len() != 1 {
                return Err(reject(NackCode::InvalidLength));
            }

            let enabled = match parameters[0] {
                0 => false,
                1 => true,
                _ => return Err(reject(NackCode::InvalidValue)),
            };

            Ok(Some(Request::OutputEnable(enabled)))
        }

        value if value == Command::SetTargets as u8 => {
            // Command + start servo + at least one complete i32 angle.
            if content.len() > crate::cobs_frame::MAX_CONTENT_SIZE
                || parameters.len() < 5
                || (parameters.len() - 1) % 4 != 0
            {
                return Err(reject(NackCode::InvalidLength));
            }

            Ok(Some(Request::SetTargets(TargetBatch {
                first_servo: parameters[0],
                angle_bytes: &parameters[1..],
            })))
        }

        _ => Err(reject(NackCode::UnknownCommand)),
    }
}

#[cfg(test)]
#[path = "test_command.rs"]
mod tests;
