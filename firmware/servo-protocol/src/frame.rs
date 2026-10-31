//! Complete-frame encoding and decoding, independent of stream parsing.

use crate::{HEADER_SIZE, Header, HeaderError};

pub const CRC_SIZE: usize = 4;
pub const MAX_PAYLOAD_SIZE: usize = 512;
pub const MAX_FRAME_SIZE: usize = HEADER_SIZE + MAX_PAYLOAD_SIZE + CRC_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Request,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    Header(HeaderError),
    InvalidLength,
    PayloadTooLarge,
    BufferTooSmall,
    InvalidResponseOpcode,
    CrcMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub header: Header,
    pub payload: &'a [u8],
}

/// Encodes one complete frame.
///
/// checksum must calculate CRC-32/ISO-HDLC independently for each call.
/// Invalid inputs must be rejected before modifying output.
pub fn encode_frame(
    header: Header,
    payload: &[u8],
    kind: FrameKind,
    output: &mut [u8],
    checksum: &mut impl FnMut(&[u8]) -> u32,
) -> Result<usize, FrameError> {
    let payload_length = frame_payload_length(header, kind)?;

    if payload.len() != payload_length {
        return Err(FrameError::InvalidLength);
    }

    let crc_offset = HEADER_SIZE + payload_length;
    let frame_length = crc_offset + CRC_SIZE;

    if output.len() < frame_length {
        return Err(FrameError::BufferTooSmall);
    }

    // All fallible input checks precede output modification.
    output[..HEADER_SIZE].copy_from_slice(&header.encode());
    output[HEADER_SIZE..crc_offset].copy_from_slice(payload);

    let crc = checksum(&output[..crc_offset]);
    output[crc_offset..frame_length].copy_from_slice(&crc.to_le_bytes());

    Ok(frame_length)
}

/// Decodes exactly one frame; trailing bytes are rejected.
///
/// The returned payload borrows from input.
/// Register permissions and value/state checks are separate.
pub fn decode_frame<'a>(
    input: &'a [u8],
    kind: FrameKind,
    checksum: &mut impl FnMut(&[u8]) -> u32,
) -> Result<Frame<'a>, FrameError> {
    if input.len() < HEADER_SIZE + CRC_SIZE {
        return Err(FrameError::InvalidLength);
    }

    let header_bytes = [input[0], input[1], input[2], input[3]];

    let header = Header::decode(&header_bytes).map_err(FrameError::Header)?;

    let payload_length = frame_payload_length(header, kind)?;
    let crc_offset = HEADER_SIZE + payload_length;
    let frame_length = crc_offset + CRC_SIZE;

    if input.len() != frame_length {
        return Err(FrameError::InvalidLength);
    }

    let received_crc = u32::from_le_bytes([
        input[crc_offset],
        input[crc_offset + 1],
        input[crc_offset + 2],
        input[crc_offset + 3],
    ]);

    if checksum(&input[..crc_offset]) != received_crc {
        return Err(FrameError::CrcMismatch);
    }

    Ok(Frame {
        header,
        payload: &input[HEADER_SIZE..crc_offset],
    })
}

/// Determines payload size from direction and opcode.
///
/// A READ request carries no payload: its length describes the response.
/// Only READ responses are supported.
fn frame_payload_length(
    header: Header,
    kind: FrameKind,
) -> Result<usize, FrameError> {
    if kind == FrameKind::Response && header.opcode != crate::Opcode::Read {
        return Err(FrameError::InvalidResponseOpcode);
    }

    let length = usize::from(header.length);

    if length > MAX_PAYLOAD_SIZE {
        return Err(FrameError::PayloadTooLarge);
    }

    if kind == FrameKind::Request && header.opcode == crate::Opcode::Read {
        Ok(0)
    } else {
        Ok(length)
    }
}

#[cfg(test)]
#[path = "test_frame.rs"]
mod tests;
