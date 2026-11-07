//! Bounded command processing over a nonblocking byte transport.

use crate::{OutputBackend, command_service::CommandService};
use hexaroam_servo_protocol::{
    ByteTransport,
    cobs_frame::CobsFrameReceiver,
    cobs_transmitter::{CobsSubmitError, CobsTransmitter},
    command::{self, Command, CommandError, Request},
    command_response,
    command_response::ResponseError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommandProgress {
    pub received: usize,
    pub transmitted: usize,

    /// Requests dispatched or rejected with NACK; excludes ignored replies.
    pub processed: usize,

    /// Complete malformed frames or encoded-overflow events.
    pub discarded: usize,

    /// A receive attempt found no available byte.
    pub receive_empty: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CommandProcessingError<E> {
    Receive(E),
    Transmit(E),
    Response(ResponseError),
    Submission(CobsSubmitError),
}

pub struct CommandProcessor<O: OutputBackend, const N: usize> {
    service: CommandService<O, N>,
    receiver: CobsFrameReceiver,
    transmitter: CobsTransmitter,
}

impl<O: OutputBackend, const N: usize> CommandProcessor<O, N> {
    pub fn new(service: CommandService<O, N>) -> Self {
        Self {
            service,
            receiver: CobsFrameReceiver::new(),
            transmitter: CobsTransmitter::new(),
        }
    }

    /// Reports bytes not yet accepted by the transmit transport.
    /// False does not confirm physical wire completion.
    pub fn response_pending(&self) -> bool {
        self.transmitter.is_pending()
    }

    /// Discards incomplete reception without changing pending responses.
    /// The caller determines timing and accounts for unread UART bytes.
    pub fn on_receive_timeout(&mut self) {
        self.receiver.on_timeout();
    }

    /// Services output once, then pending TX, then bounded RX.
    ///
    /// A newly generated response is transmitted on a later poll.
    /// Pending transmission blocks reception, not output servicing.
    /// Invalid COBS/CRC frames are discarded without a response.
    /// Transport errors preserve earlier progress and are returned.
    ///
    /// Byte budgets bound transport work, not backend execution time.
    pub fn poll<T: ByteTransport>(
        &mut self,
        transport: &mut T,
        receive_budget: usize,
        transmit_budget: usize,
        checksum: &mut impl FnMut(&[u8]) -> u32,
    ) -> Result<CommandProgress, CommandProcessingError<T::Error>> {
        // Backend servicing remains reachable during backpressure and errors.
        self.service.service();

        let transmitted = self
            .transmitter
            .service(transport, transmit_budget)
            .map_err(CommandProcessingError::Transmit)?;

        let mut progress = CommandProgress {
            transmitted,
            ..CommandProgress::default()
        };

        if self.transmitter.is_pending() {
            return Ok(progress);
        }

        while progress.received < receive_budget {
            let byte = match transport
                .try_read()
                .map_err(CommandProcessingError::Receive)?
            {
                Some(byte) => byte,
                None => {
                    progress.receive_empty = true;
                    return Ok(progress);
                }
            };

            progress.received += 1;

            let content = match self.receiver.push(byte, checksum) {
                Ok(Some(content)) => content,
                Ok(None) => continue,
                Err(_) => {
                    // Framing and CRC failures never reach the service.
                    progress.discarded += 1;
                    continue;
                }
            };

            // Six bytes accommodate the largest current response:
            // ACK + original command + u32 status.
            let mut response = [0; 6];

            let response_length = match command::decode(content) {
                Ok(None) => {
                    // Incoming ACK/NACK must not trigger response loops.
                    continue;
                }

                Err(CommandError::EmptyInput) => {
                    // Normally excluded by the frame receiver's minimum size.
                    progress.discarded += 1;
                    continue;
                }

                Err(CommandError::Rejected { command, code }) => {
                    command_response::encode_nack(command, code, &mut response)
                }

                Ok(Some(Request::GetStatus)) => {
                    command_response::encode_status(
                        self.service.status(),
                        &mut response,
                    )
                }

                Ok(Some(Request::OutputEnable(enabled))) => {
                    match self.service.set_enabled(enabled) {
                        Ok(()) => command_response::encode_ack(
                            Command::OutputEnable,
                            &mut response,
                        ),
                        Err(code) => command_response::encode_nack(
                            Command::OutputEnable as u8,
                            code,
                            &mut response,
                        ),
                    }
                }

                Ok(Some(Request::SetTargets(batch))) => {
                    match self.service.set_targets(batch) {
                        Ok(()) => command_response::encode_ack(
                            Command::SetTargets,
                            &mut response,
                        ),
                        Err(code) => command_response::encode_nack(
                            Command::SetTargets as u8,
                            code,
                            &mut response,
                        ),
                    }
                }
            }
            .map_err(CommandProcessingError::Response)?;

            progress.processed += 1;

            self.transmitter
                .submit(&response[..response_length], checksum)
                .map_err(CommandProcessingError::Submission)?;

            // The next poll transmits this response before receiving more.
            return Ok(progress);
        }

        Ok(progress)
    }
}

#[cfg(test)]
#[path = "test_command_processor.rs"]
mod tests;
