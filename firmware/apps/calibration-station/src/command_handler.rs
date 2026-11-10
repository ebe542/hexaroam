//! Connects station requests to pulse control.
//!
//! Input and output contain command bytes and parameters only.
//! COBS, CRC and transport remain outside this module.

use crate::{
    command::{self, Command, Request},
    control::{CalibrationControl, ControlError, PulseBackend},
};
use hexaroam_servo_protocol::command::{CommandError, NackCode};

pub struct Response {
    bytes: [u8; 10],
    length: usize,
}

impl Response {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }

    fn ack(command: Command) -> Self {
        let mut bytes = [0; 10];
        bytes[0] = 0x80;
        bytes[1] = command as u8;

        Self { bytes, length: 2 }
    }

    fn nack(command: u8, code: NackCode) -> Self {
        let mut bytes = [0; 10];
        bytes[0] = 0x81;
        bytes[1] = command;
        bytes[2] = code as u8;

        Self { bytes, length: 3 }
    }

    fn status(flags: u32, pulse_width_us: Option<u32>) -> Self {
        let mut bytes = [0; 10];
        bytes[0] = 0x80;
        bytes[1] = Command::GetStatus as u8;
        bytes[2..6].copy_from_slice(&flags.to_le_bytes());
        bytes[6..10].copy_from_slice(
            &pulse_width_us
                .unwrap_or(0)
                .to_le_bytes(),
        );

        Self { bytes, length: 10 }
    }
}

/// Returns ACK/NACK content for requests.
/// Empty input and incoming ACK/NACK produce no response.
/// Backend errors are translated into protocol NACK codes.
pub fn process<B: PulseBackend>(
    control: &mut CalibrationControl<B>,
    content: &[u8],
) -> Option<Response> {
    let request = match command::decode(content) {
        Ok(Some(request)) => request,
        Ok(None) | Err(CommandError::EmptyInput) => return None,
        Err(CommandError::Rejected { command, code }) => {
            return Some(Response::nack(command, code));
        }
    };

    let (command, result) = match request {
        Request::GetStatus => {
            return Some(Response::status(
                control.status(),
                control.pulse_width_us(),
            ));
        }
        Request::OutputEnable(enabled) => (
            Command::OutputEnable,
            control.set_enabled(enabled),
        ),
        Request::SetPulse(pulse_width_us) => (
            Command::SetPulse,
            control.set_pulse(pulse_width_us),
        ),
    };

    Some(match result {
        Ok(()) => Response::ack(command),
        Err(error) => {
            let code = match error {
                ControlError::OutOfRange => NackCode::InvalidValue,
                ControlError::MissingPulse => NackCode::InvalidState,
                ControlError::Busy => NackCode::Busy,
                ControlError::OutputError => NackCode::OutputError,
            };

            Response::nack(command as u8, code)
        }
    })
}

#[cfg(test)]
#[path = "test_command_handler.rs"]
mod tests;
