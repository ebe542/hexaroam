//! Nonblocking transmission of one complete COBS/CRC32 frame.

use crate::{
    ByteTransport, ResponseBuffer, ResponseQueueError,
    cobs_frame::{CobsFrameError, MAX_WIRE_SIZE},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobsSubmitError {
    Busy,
    Frame(CobsFrameError),
    Queue(ResponseQueueError),
}

pub struct CobsTransmitter {
    pending: ResponseBuffer<MAX_WIRE_SIZE>,
}

impl Default for CobsTransmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl CobsTransmitter {
    pub const fn new() -> Self {
        Self {
            pending: ResponseBuffer::new(),
        }
    }

    /// Encodes and copies one complete frame.
    ///
    /// Busy takes precedence over content validation and checksum work.
    /// Errors preserve the pending frame and its transmission progress.
    /// Input storage is no longer needed after this call returns.
    pub fn submit(
        &mut self,
        content: &[u8],
        checksum: &mut impl FnMut(&[u8]) -> u32,
    ) -> Result<(), CobsSubmitError> {
        if self.pending.is_pending() {
            return Err(CobsSubmitError::Busy);
        }

        let mut wire = [0; MAX_WIRE_SIZE];

        let length = crate::cobs_frame::encode(content, &mut wire, checksum)
            .map_err(CobsSubmitError::Frame)?;

        self.pending
            .queue(&wire[..length])
            .map_err(CobsSubmitError::Queue)
    }

    /// Reports bytes not yet accepted by the transport.
    ///
    /// False does not guarantee physical wire completion.
    pub fn is_pending(&self) -> bool {
        self.pending.is_pending()
    }

    /// Sends at most byte_budget bytes without waiting or receiving.
    ///
    /// Backpressure preserves the unsent suffix. A transport error
    /// preserves the failed byte and all following bytes.
    pub fn service<T: ByteTransport>(
        &mut self,
        transport: &mut T,
        byte_budget: usize,
    ) -> Result<usize, T::Error> {
        self.pending
            .service(transport, byte_budget)
    }
}

#[cfg(test)]
#[path = "test_cobs_transmitter.rs"]
mod tests;
