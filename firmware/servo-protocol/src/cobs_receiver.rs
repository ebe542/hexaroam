//! Bounded COBS frame reception without CRC or command validation.

use crate::cobs::{self, CobsError};

pub const MAX_ENCODED_SIZE: usize = 256;
pub const MAX_DECODED_SIZE: usize = 254;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobsReceiveError {
    EncodedTooLong,
    DecodedTooLong,
    InvalidEncoding,
}

pub struct CobsReceiver {
    encoded: [u8; MAX_ENCODED_SIZE],
    decoded: [u8; MAX_DECODED_SIZE],
    encoded_length: usize,
    synchronized: bool,
}

impl Default for CobsReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl CobsReceiver {
    pub const fn new() -> Self {
        Self {
            encoded: [0; MAX_ENCODED_SIZE],
            decoded: [0; MAX_DECODED_SIZE],
            encoded_length: 0,
            synchronized: false,
        }
    }

    /// Consumes one wire byte.
    ///
    /// Initially ignores bytes until the first zero delimiter.
    /// Consecutive delimiters produce no frame.
    ///
    /// Overflow discards the frame and ignores bytes until the next zero.
    /// Decode errors at a delimiter preserve synchronization.
    ///
    /// The returned bytes borrow internal storage. Empty decoded content
    /// is allowed here; minimum protocol size is checked by the next layer.
    pub fn push(
        &mut self,
        byte: u8,
    ) -> Result<Option<&[u8]>, CobsReceiveError> {
        if byte == 0 {
            if !self.synchronized {
                self.encoded_length = 0;
                self.synchronized = true;
                return Ok(None);
            }

            if self.encoded_length == 0 {
                return Ok(None);
            }

            // The delimiter closes this frame and establishes the boundary
            // for the next one, including after a decoding error.
            let encoded_length = self.encoded_length;
            self.encoded_length = 0;

            let decoded_length = cobs::decode(
                &self.encoded[..encoded_length],
                &mut self.decoded,
            )
            .map_err(|error| match error {
                CobsError::OutputTooSmall => CobsReceiveError::DecodedTooLong,
                CobsError::InvalidEncoding => CobsReceiveError::InvalidEncoding,
            })?;

            return Ok(Some(&self.decoded[..decoded_length]));
        }

        if !self.synchronized {
            return Ok(None);
        }

        if self.encoded_length == self.encoded.len() {
            // Ignore the remainder until a delimiter establishes a new
            // boundary. Report overflow once, not for every following byte.
            self.encoded_length = 0;
            self.synchronized = false;
            return Err(CobsReceiveError::EncodedTooLong);
        }

        self.encoded[self.encoded_length] = byte;
        self.encoded_length += 1;

        Ok(None)
    }

    /// Discards any partial frame and waits for a new zero delimiter.
    ///
    /// The caller determines when reception has timed out.
    pub fn on_timeout(&mut self) {
        self.encoded_length = 0;
        self.synchronized = false;
    }
}

#[cfg(test)]
#[path = "test_cobs_receiver.rs"]
mod tests;
