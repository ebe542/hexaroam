//! Bounded station command processing over a nonblocking transport.

use crate::command_handler;
use crate::control::{CalibrationControl, PulseBackend};
use hexaroam_servo_protocol::{
    ByteTransport,
    cobs_frame::CobsFrameReceiver,
    cobs_transmitter::{CobsSubmitError, CobsTransmitter},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub received: usize,
    pub transmitted: usize,
    pub processed: usize,
    pub discarded: usize,
    pub receive_empty: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProcessingError<E> {
    Receive(E),
    Transmit(E),
    Submission(CobsSubmitError),
}

pub struct FrameProcessor<B: PulseBackend> {
    control: CalibrationControl<B>,
    receiver: CobsFrameReceiver,
    transmitter: CobsTransmitter,
}

impl<B: PulseBackend> FrameProcessor<B> {
    pub fn new(control: CalibrationControl<B>) -> Self {
        Self {
            control,
            receiver: CobsFrameReceiver::new(),
            transmitter: CobsTransmitter::new(),
        }
    }

    pub fn status(&self) -> u32 {
        self.control.status()
    }

    /// Reports bytes not yet accepted by the transport.
    /// False does not confirm physical wire completion.
    pub fn response_pending(&self) -> bool {
        self.transmitter.is_pending()
    }

    /// Discards incomplete reception without changing pending responses.
    /// The caller decides when a receive timeout has occurred.
    pub fn on_receive_timeout(&mut self) {
        self.receiver.on_timeout();
    }

    /// Services output, pending transmission, then bounded reception.
    ///
    /// A new response is transmitted on a subsequent poll.
    /// Pending transmission blocks reception, not output servicing.
    /// Invalid frames are discarded before command dispatch.
    /// Transport errors preserve partial reception or pending transmission.
    pub fn poll<T: ByteTransport>(
        &mut self,
        transport: &mut T,
        receive_budget: usize,
        transmit_budget: usize,
        checksum: &mut impl FnMut(&[u8]) -> u32,
    ) -> Result<Progress, ProcessingError<T::Error>> {
        // Keep output servicing reachable during backpressure and errors.
        self.control.service();

        let transmitted = self
            .transmitter
            .service(transport, transmit_budget)
            .map_err(ProcessingError::Transmit)?;

        let mut progress = Progress {
            transmitted,
            ..Progress::default()
        };

        if self.transmitter.is_pending() {
            return Ok(progress);
        }

        while progress.received < receive_budget {
            let byte = match transport
                .try_read()
                .map_err(ProcessingError::Receive)?
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
                    // Invalid framing or CRC must not reach the handler.
                    progress.discarded += 1;
                    continue;
                }
            };

            let Some(response) =
                command_handler::process(&mut self.control, content)
            else {
                // Incoming ACK/NACK content must not cause response loops.
                continue;
            };

            self.transmitter
                .submit(response.as_bytes(), checksum)
                .map_err(ProcessingError::Submission)?;

            progress.processed += 1;

            // Send this response before processing another request.
            return Ok(progress);
        }

        Ok(progress)
    }
}

#[cfg(test)]
#[path = "test_frame_processor.rs"]
mod tests;
