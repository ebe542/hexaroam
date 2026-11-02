//! Bounded transmission of one encoded response.

use crate::ByteTransport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseQueueError {
    Busy,
    Empty,
    TooLarge,
}

pub struct ResponseBuffer<const CAPACITY: usize> {
    buffer: [u8; CAPACITY],
    length: usize,
    offset: usize,
}

impl<const CAPACITY: usize> ResponseBuffer<CAPACITY> {
    pub const fn new() -> Self {
        Self {
            buffer: [0; CAPACITY],
            length: 0,
            offset: 0,
        }
    }

    /// Copies one complete encoded response into the empty buffer.
    ///
    /// Does not validate framing or CRC.
    /// Errors preserve the previously queued response and its progress.
    /// Busy takes precedence over Empty and TooLarge.
    pub fn queue(&mut self, bytes: &[u8]) -> Result<(), ResponseQueueError> {
        if self.is_pending() {
            return Err(ResponseQueueError::Busy);
        }

        if bytes.is_empty() {
            return Err(ResponseQueueError::Empty);
        }

        if bytes.len() > CAPACITY {
            return Err(ResponseQueueError::TooLarge);
        }

        // All rejection checks precede changes to data or progress.
        self.buffer[..bytes.len()].copy_from_slice(bytes);
        self.length = bytes.len();
        self.offset = 0;

        Ok(())
    }

    pub fn is_pending(&self) -> bool {
        self.remaining() != 0
    }

    /// Returns bytes not yet accepted by the transport.
    pub fn remaining(&self) -> usize {
        self.length - self.offset
    }

    /// Sends at most byte_budget bytes without waiting.
    ///
    /// Stops immediately when the transport cannot accept a byte.
    /// Successful bytes advance the offset exactly once.
    /// An error preserves the unsent suffix, including the failed byte.
    ///
    /// Returns the number of bytes accepted during a successful call.
    /// On error, earlier progress remains committed.
    pub fn service<T: ByteTransport>(
        &mut self,
        transport: &mut T,
        byte_budget: usize,
    ) -> Result<usize, T::Error> {
        let mut accepted = 0;

        while accepted < byte_budget && self.is_pending() {
            if !transport.try_write(self.buffer[self.offset])? {
                break;
            }

            // Advance only after this byte was accepted.
            // Earlier progress remains committed if a later write fails.
            self.offset += 1;
            accepted += 1;
        }

        if !self.is_pending() {
            self.length = 0;
            self.offset = 0;
        }

        Ok(accepted)
    }
}

#[cfg(test)]
#[path = "test_response_buffer.rs"]
mod tests;
