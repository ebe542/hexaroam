//! COBS framing with CRC-32/ISO-HDLC.
//!
//! Content includes the command byte but excludes CRC and delimiters.
//! Multibyte CRC values use little endian.

use crate::{
    cobs::CobsError,
    cobs_receiver::{CobsReceiveError, CobsReceiver},
};

pub const MAX_CONTENT_SIZE: usize = 250;
pub const MAX_WIRE_SIZE: usize = 258;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobsFrameError {
    EmptyContent,
    ContentTooLong,
    OutputTooSmall,
    TooShort,
    CrcMismatch,
    Encoding(CobsError),
    Receive(CobsReceiveError),
}

/// Encodes content, CRC32 and both zero delimiters.
///
/// Returns the number of bytes written. The output suffix remains
/// unchanged on success; output contents are unspecified after error.
/// The checksum callback must calculate CRC-32/ISO-HDLC independently
/// for each invocation.
pub fn encode(
    content: &[u8],
    output: &mut [u8],
    checksum: &mut impl FnMut(&[u8]) -> u32,
) -> Result<usize, CobsFrameError> {
    if content.is_empty() {
        return Err(CobsFrameError::EmptyContent);
    }

    if content.len() > MAX_CONTENT_SIZE {
        return Err(CobsFrameError::ContentTooLong);
    }

    let mut decoded = [0; MAX_CONTENT_SIZE + 4];
    decoded[..content.len()].copy_from_slice(content);

    let crc = checksum(content).to_le_bytes();
    let decoded_length = content.len() + crc.len();
    decoded[content.len()..decoded_length].copy_from_slice(&crc);

    // Encode separately so exact-size destination buffers are accepted.
    let mut encoded = [0; crate::cobs_receiver::MAX_ENCODED_SIZE];
    let encoded_length =
        crate::cobs::encode(&decoded[..decoded_length], &mut encoded)
            .map_err(CobsFrameError::Encoding)?;

    let wire_length = encoded_length + 2;

    if output.len() < wire_length {
        return Err(CobsFrameError::OutputTooSmall);
    }

    output[0] = 0;
    output[1..wire_length - 1].copy_from_slice(&encoded[..encoded_length]);
    output[wire_length - 1] = 0;

    Ok(wire_length)
}

pub struct CobsFrameReceiver {
    receiver: CobsReceiver,
}

impl Default for CobsFrameReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl CobsFrameReceiver {
    pub const fn new() -> Self {
        Self {
            receiver: CobsReceiver::new(),
        }
    }

    /// Returns CRC-validated content without CRC or delimiters.
    ///
    /// CRC is calculated only for complete decoded frames with at
    /// least one content byte and four CRC bytes.
    ///
    /// Unknown commands are accepted here; command validation belongs
    /// to the next layer. Returned content borrows internal storage.
    pub fn push(
        &mut self,
        byte: u8,
        checksum: &mut impl FnMut(&[u8]) -> u32,
    ) -> Result<Option<&[u8]>, CobsFrameError> {
        let Some(decoded) = self
            .receiver
            .push(byte)
            .map_err(CobsFrameError::Receive)?
        else {
            return Ok(None);
        };

        // At least one command byte and four CRC bytes are required.
        if decoded.len() < 5 {
            return Err(CobsFrameError::TooShort);
        }

        let content_length = decoded.len() - 4;
        let (content, crc_bytes) = decoded.split_at(content_length);

        let received_crc = u32::from_le_bytes([
            crc_bytes[0],
            crc_bytes[1],
            crc_bytes[2],
            crc_bytes[3],
        ]);

        if checksum(content) != received_crc {
            return Err(CobsFrameError::CrcMismatch);
        }

        Ok(Some(content))
    }

    pub fn on_timeout(&mut self) {
        self.receiver.on_timeout();
    }
}

#[cfg(test)]
#[path = "test_cobs_frame.rs"]
mod tests;
