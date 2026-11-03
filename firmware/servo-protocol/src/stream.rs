//! Incremental byte-stream parsing without hardware dependencies.

use crate::validation::validate_regions;
use crate::{
    CRC_SIZE, Frame, FrameKind, HEADER_SIZE, Header, MAX_FRAME_SIZE,
    MAX_PAYLOAD_SIZE, Opcode, RegisterRegion, ValidationError, validate_header,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserError {
    InvalidMap,
    BufferFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedFrame<'a> {
    pub frame: Frame<'a>,
    pub validation: Result<(), ValidationError>,
}

pub struct StreamParser<'a> {
    kind: FrameKind,
    regions: &'a [RegisterRegion],
    buffer: [u8; MAX_FRAME_SIZE],
    len: usize,
    consumed: usize,
}

impl<'a> StreamParser<'a> {
    pub fn new(
        kind: FrameKind,
        regions: &'a [RegisterRegion],
    ) -> Result<Self, ParserError> {
        validate_regions(regions).map_err(|_| ParserError::InvalidMap)?;

        Ok(Self {
            kind,
            regions,
            buffer: [0; MAX_FRAME_SIZE],
            len: 0,
            consumed: 0,
        })
    }

    /// Adds one byte without overwriting existing buffered data.
    ///
    /// On BufferFull, the new byte is not accepted.
    pub fn push(&mut self, byte: u8) -> Result<(), ParserError> {
        self.retire_consumed();

        if self.len == self.buffer.len() {
            return Err(ParserError::BufferFull);
        }

        self.buffer[self.len] = byte;
        self.len += 1;

        Ok(())
    }

    /// Returns the next complete CRC-valid frame.
    ///
    /// Request indices and lengths must be plausible before collecting a frame.
    /// Plausible requests can carry an access error for the register service.
    /// No returned frame permits hardware changes without payload/state checks.
    pub fn next_frame(
        &mut self,
        checksum: &mut impl FnMut(&[u8]) -> u32,
    ) -> Option<ParsedFrame<'_>> {
        self.retire_consumed();

        loop {
            if self.len == 0 {
                return None;
            }

            let opcode = match Opcode::from_byte(self.buffer[0]) {
                Ok(opcode) => opcode,
                Err(_) => {
                    self.discard_prefix(1);
                    continue;
                }
            };

            let response_opcode = matches!(opcode, Opcode::Ack | Opcode::Nack);

            let valid_direction = match self.kind {
                FrameKind::Request => !response_opcode,
                FrameKind::Response => response_opcode,
            };

            if !valid_direction {
                self.discard_prefix(1);
                continue;
            }

            if self.len < HEADER_SIZE {
                return None;
            }

            let header = Header {
                opcode,
                index: self.buffer[1],
                length: u16::from_le_bytes([self.buffer[2], self.buffer[3]]),
            };

            if self.kind == FrameKind::Request {
                // Validate index and length independently of write permissions.
                // All current regions support READ, so a synthetic READ header
                // checks their widths and boundaries without authorizing writes.
                let plausibility_header = Header {
                    opcode: Opcode::Read,
                    ..header
                };

                if validate_header(&plausibility_header, self.regions).is_err()
                    || usize::from(header.length) > MAX_PAYLOAD_SIZE
                {
                    // Reject impossible candidates before collecting payload
                    // or CRC, preserving overlapping possible frame starts.
                    self.discard_prefix(1);
                    continue;
                }
            }

            let read_request =
                self.kind == FrameKind::Request && opcode == Opcode::Read;

            // Responses describe an outcome, not a new register access.
            // An empty ACK or a NACK for an unknown index is therefore valid.
            let validation = match self.kind {
                FrameKind::Request => validate_header(&header, self.regions),
                FrameKind::Response => Ok(()),
            };

            if self.kind == FrameKind::Response
                && opcode == Opcode::Nack
                && header.length != 1
            {
                self.discard_prefix(1);
                continue;
            }

            let payload_length = if read_request {
                0
            } else {
                if usize::from(header.length) > MAX_PAYLOAD_SIZE {
                    // Oversized payloads cannot fit in the parser buffer.
                    // Preserve overlapping possible frame starts.
                    self.discard_prefix(1);
                    continue;
                }

                // A plausible request with denied access still needs a CRC
                // check before the service may send a NACK response.

                usize::from(header.length)
            };

            let crc_offset = HEADER_SIZE + payload_length;
            let frame_length = crc_offset + CRC_SIZE;

            if self.len < frame_length {
                return None;
            }

            let received_crc = u32::from_le_bytes([
                self.buffer[crc_offset],
                self.buffer[crc_offset + 1],
                self.buffer[crc_offset + 2],
                self.buffer[crc_offset + 3],
            ]);

            if checksum(&self.buffer[..crc_offset]) != received_crc {
                self.discard_prefix(1);
                continue;
            }

            // Keep bytes in place while the returned payload borrows them.
            // Rust prevents another mutable parser call during that borrow.
            self.consumed = frame_length;

            return Some(ParsedFrame {
                frame: Frame {
                    header,
                    payload: &self.buffer[HEADER_SIZE..crc_offset],
                },
                validation,
            });
        }
    }

    /// Discards the remaining incomplete input.
    ///
    /// The caller must drain next_frame() before delivering this event.
    pub fn on_timeout(&mut self) {
        // Reset the logical contents; clearing the backing bytes is unnecessary.
        self.len = 0;
        self.consumed = 0;
    }

    fn retire_consumed(&mut self) {
        if self.consumed != 0 {
            let count = self.consumed;
            self.consumed = 0;
            self.discard_prefix(count);
        }
    }

    fn discard_prefix(&mut self, count: usize) {
        self.buffer
            .copy_within(count..self.len, 0);
        self.len -= count;
    }
}

#[cfg(test)]
#[path = "test_stream.rs"]
mod tests;
