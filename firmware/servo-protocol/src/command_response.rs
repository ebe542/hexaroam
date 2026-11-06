//! Response content encoding before CRC32 and COBS framing.

use crate::command::{Command, NackCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseError {
    OutputTooSmall,
    MissingStatus,
}

/// Writes ACK, GET_STATUS and the little-endian status word.
/// Errors leave output unchanged; success preserves the unused suffix.
pub fn encode_status(
    status: u32,
    output: &mut [u8],
) -> Result<usize, ResponseError> {
    if output.len() < 6 {
        return Err(ResponseError::OutputTooSmall);
    }

    output[0] = 0x80;
    output[1] = Command::GetStatus as u8;
    output[2..6].copy_from_slice(&status.to_le_bytes());

    Ok(6)
}

/// Writes ACK and the accepted command.
/// GET_STATUS requires encode_status and returns MissingStatus here.
/// MissingStatus takes precedence over output capacity.
pub fn encode_ack(
    command: Command,
    output: &mut [u8],
) -> Result<usize, ResponseError> {
    if command == Command::GetStatus {
        return Err(ResponseError::MissingStatus);
    }

    if output.len() < 2 {
        return Err(ResponseError::OutputTooSmall);
    }

    output[0] = 0x80;
    output[1] = command as u8;

    Ok(2)
}

/// Writes NACK, the original command byte and its error code.
/// The caller must not respond to incoming ACK/NACK messages.
/// Errors leave output unchanged; success preserves the unused suffix.
pub fn encode_nack(
    command: u8,
    code: NackCode,
    output: &mut [u8],
) -> Result<usize, ResponseError> {
    if output.len() < 3 {
        return Err(ResponseError::OutputTooSmall);
    }

    output[0] = 0x81;
    output[1] = command;
    output[2] = code as u8;

    Ok(3)
}

#[cfg(test)]
#[path = "test_command_response.rs"]
mod tests;
