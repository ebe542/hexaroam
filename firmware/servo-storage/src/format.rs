//! Binary representation of servo calibrations.

use hexaroam_servo_control::{Angle, Calibration, CalibrationError};

use crate::Checksum;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    BufferTooSmall,
    TooManyEntries,
    InvalidMagic,
    InvalidLength,
    CrcMismatch,
    UnknownServo(u32),
    DuplicateServo(u32),
    InvalidCalibration { id: u32, error: CalibrationError },
}

const MAGIC: [u8; 2] = [0x48, 0x01];
const HEADER_SIZE: usize = MAGIC.len() + size_of::<u16>();
const ENTRY_SIZE: usize = 20;
const CRC_SIZE: usize = 4;

/// Encodes present calibrations in ascending ServoId order.
///
/// Returns the number of bytes written, including header and CRC.
/// Bytes beyond the returned length remain unchanged.
pub fn encode<const N: usize>(
    calibrations: &[Option<Calibration>; N],
    output: &mut [u8],
    checksum: &mut impl Checksum,
) -> Result<usize, FormatError> {
    let entry_count = calibrations.iter().flatten().count();

    let payload_length = entry_count
        .checked_mul(ENTRY_SIZE)
        .and_then(|length| u16::try_from(length).ok())
        .ok_or(FormatError::TooManyEntries)?;

    // Validate IDs before modifying the output buffer.
    for (id, calibration) in calibrations.iter().enumerate() {
        if calibration.is_some() && u32::try_from(id).is_err() {
            return Err(FormatError::TooManyEntries);
        }
    }

    let crc_offset = HEADER_SIZE + usize::from(payload_length);
    let total_length = crc_offset + CRC_SIZE;

    if output.len() < total_length {
        return Err(FormatError::BufferTooSmall);
    }

    output[..MAGIC.len()].copy_from_slice(&MAGIC);
    output[MAGIC.len()..HEADER_SIZE]
        .copy_from_slice(&payload_length.to_le_bytes());

    let mut offset = HEADER_SIZE;

    for (id, calibration) in calibrations.iter().enumerate() {
        let Some(calibration) = calibration else {
            continue;
        };

        // Conversion was checked before writing.
        let id = id as u32;

        let fields = [
            id.to_le_bytes(),
            calibration
                .lower_angle()
                .as_centidegrees()
                .to_le_bytes(),
            calibration
                .lower_pulse_us()
                .to_le_bytes(),
            calibration
                .upper_angle()
                .as_centidegrees()
                .to_le_bytes(),
            calibration
                .upper_pulse_us()
                .to_le_bytes(),
        ];

        for field in fields {
            output[offset..offset + 4].copy_from_slice(&field);
            offset += 4;
        }
    }

    let crc = checksum.checksum(&output[..crc_offset]);
    output[crc_offset..total_length].copy_from_slice(&crc.to_le_bytes());

    Ok(total_length)
}

/// Decodes one exact record, without flash-slot padding.
///
/// Missing entries remain None. Input order is arbitrary.
/// No partial result is returned on error.
pub fn decode<const N: usize>(
    input: &[u8],
    checksum: &mut impl Checksum,
) -> Result<[Option<Calibration>; N], FormatError> {
    if input.len() < HEADER_SIZE + CRC_SIZE {
        return Err(FormatError::InvalidLength);
    }

    if input[..MAGIC.len()] != MAGIC {
        return Err(FormatError::InvalidMagic);
    }

    let length_offset = MAGIC.len();
    let payload_length = usize::from(u16::from_le_bytes([
        input[length_offset],
        input[length_offset + 1],
    ]));

    let crc_offset = HEADER_SIZE + payload_length;
    let total_length = crc_offset + CRC_SIZE;

    if payload_length % ENTRY_SIZE != 0 || input.len() != total_length {
        return Err(FormatError::InvalidLength);
    }

    let stored_crc = u32::from_le_bytes([
        input[crc_offset],
        input[crc_offset + 1],
        input[crc_offset + 2],
        input[crc_offset + 3],
    ]);

    if checksum.checksum(&input[..crc_offset]) != stored_crc {
        return Err(FormatError::CrcMismatch);
    }

    let mut calibrations = [None; N];

    for entry in input[HEADER_SIZE..crc_offset].chunks_exact(ENTRY_SIZE) {
        let read_field = |offset: usize| {
            [
                entry[offset],
                entry[offset + 1],
                entry[offset + 2],
                entry[offset + 3],
            ]
        };

        let id = u32::from_le_bytes(read_field(0));
        let index =
            usize::try_from(id).map_err(|_| FormatError::UnknownServo(id))?;

        let slot = calibrations
            .get_mut(index)
            .ok_or(FormatError::UnknownServo(id))?;

        if slot.is_some() {
            return Err(FormatError::DuplicateServo(id));
        }

        let lower_angle =
            Angle::from_centidegrees(i32::from_le_bytes(read_field(4)));
        let lower_pulse_us = u32::from_le_bytes(read_field(8));
        let upper_angle =
            Angle::from_centidegrees(i32::from_le_bytes(read_field(12)));
        let upper_pulse_us = u32::from_le_bytes(read_field(16));

        let calibration = Calibration::new(
            lower_angle,
            lower_pulse_us,
            upper_angle,
            upper_pulse_us,
        )
        .map_err(|error| FormatError::InvalidCalibration { id, error })?;

        *slot = Some(calibration);
    }

    Ok(calibrations)
}

#[cfg(test)]
#[path = "test_format.rs"]
mod tests;
