//! Binary representation of one fixed-size journal slot.

use hexaroam_servo_control::Calibration;

use crate::{Checksum, FormatError};

pub const SLOT_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    Empty,
    InvalidMagic,
    InvalidLength,
    PayloadTooLarge,
    CrcMismatch,
    Format(FormatError),
}

const MAGIC: [u8; 2] = [0x4A, 0x01];
const LENGTH_OFFSET: usize = MAGIC.len();
const SEQUENCE_OFFSET: usize = LENGTH_OFFSET + size_of::<u16>();
const HEADER_SIZE: usize = SEQUENCE_OFFSET + size_of::<u64>();
const CRC_SIZE: usize = size_of::<u32>();
const MIN_PAYLOAD_SIZE: usize = 8;
const MAX_PAYLOAD_SIZE: usize = SLOT_SIZE - HEADER_SIZE - CRC_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalRecord<const N: usize> {
    pub sequence: u64,
    pub calibrations: [Option<Calibration>; N],
}

/// Encodes a complete slot, filling unused bytes with 0xFF.
pub fn encode_slot<const N: usize>(
    sequence: u64,
    calibrations: &[Option<Calibration>; N],
    output: &mut [u8; SLOT_SIZE],
    checksum: &mut impl Checksum,
) -> Result<(), SlotError> {
    // Build locally so errors leave the caller's buffer unchanged.
    let mut candidate = [0xFF; SLOT_SIZE];

    let payload_length = crate::encode(
        calibrations,
        &mut candidate[HEADER_SIZE..SLOT_SIZE - CRC_SIZE],
        checksum,
    )
    .map_err(|error| match error {
        FormatError::BufferTooSmall => SlotError::PayloadTooLarge,
        other => SlotError::Format(other),
    })?;

    candidate[..MAGIC.len()].copy_from_slice(&MAGIC);
    candidate[LENGTH_OFFSET..SEQUENCE_OFFSET]
        .copy_from_slice(&(payload_length as u16).to_le_bytes());
    candidate[SEQUENCE_OFFSET..HEADER_SIZE]
        .copy_from_slice(&sequence.to_le_bytes());

    let crc_offset = HEADER_SIZE + payload_length;
    let crc = checksum.checksum(&candidate[..crc_offset]);

    candidate[crc_offset..crc_offset + CRC_SIZE]
        .copy_from_slice(&crc.to_le_bytes());

    *output = candidate;

    Ok(())
}

/// Validates both journal and calibration records.
///
/// A completely erased slot returns SlotError::Empty.
/// Padding after the journal CRC is ignored.
pub fn decode_slot<const N: usize>(
    input: &[u8; SLOT_SIZE],
    checksum: &mut impl Checksum,
) -> Result<JournalRecord<N>, SlotError> {
    if input.iter().all(|&byte| byte == 0xFF) {
        return Err(SlotError::Empty);
    }

    if input[..MAGIC.len()] != MAGIC {
        return Err(SlotError::InvalidMagic);
    }

    let payload_length = usize::from(u16::from_le_bytes([
        input[LENGTH_OFFSET],
        input[LENGTH_OFFSET + 1],
    ]));

    if !(MIN_PAYLOAD_SIZE..=MAX_PAYLOAD_SIZE).contains(&payload_length) {
        return Err(SlotError::InvalidLength);
    }

    let crc_offset = HEADER_SIZE + payload_length;
    let stored_crc = u32::from_le_bytes([
        input[crc_offset],
        input[crc_offset + 1],
        input[crc_offset + 2],
        input[crc_offset + 3],
    ]);

    if checksum.checksum(&input[..crc_offset]) != stored_crc {
        return Err(SlotError::CrcMismatch);
    }

    let sequence = u64::from_le_bytes([
        input[SEQUENCE_OFFSET],
        input[SEQUENCE_OFFSET + 1],
        input[SEQUENCE_OFFSET + 2],
        input[SEQUENCE_OFFSET + 3],
        input[SEQUENCE_OFFSET + 4],
        input[SEQUENCE_OFFSET + 5],
        input[SEQUENCE_OFFSET + 6],
        input[SEQUENCE_OFFSET + 7],
    ]);

    let calibrations = crate::decode::<N>(
        &input[HEADER_SIZE..crc_offset],
        checksum,
    )
    .map_err(SlotError::Format)?;

    Ok(JournalRecord {
        sequence,
        calibrations,
    })
}

#[cfg(test)]
#[path = "test_journal_slot.rs"]
mod tests;
