//! Calibration-station requests after COBS and CRC validation.

use hexaroam_servo_protocol::command::{CommandError, NackCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    GetStatus = 0x01,
    OutputEnable = 0x02,
    SetPulse = 0x03,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    GetStatus,
    OutputEnable(bool),
    SetPulse(u32),
}

/// Checks request structure; pulse limits belong to the service.
/// Incoming ACK/NACK content is ignored, as in the controller decoder.
pub fn decode(content: &[u8]) -> Result<Option<Request>, CommandError> {
    let (&command, parameters) = content
        .split_first()
        .ok_or(CommandError::EmptyInput)?;

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

        value if value == Command::SetPulse as u8 => {
            if parameters.len() != 4 {
                return Err(reject(NackCode::InvalidLength));
            }

            let pulse_width_us = u32::from_le_bytes([
                parameters[0],
                parameters[1],
                parameters[2],
                parameters[3],
            ]);

            Ok(Some(Request::SetPulse(pulse_width_us)))
        }

        _ => Err(reject(NackCode::UnknownCommand)),
    }
}

#[cfg(test)]
#[path = "test_command.rs"]
mod tests;
