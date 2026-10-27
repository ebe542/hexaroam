extern crate std;

use std::vec::Vec;

use hexaroam_servo_control::{Angle, Calibration, CalibrationError};

use super::{FormatError, decode, encode};
use crate::{Checksum, SoftwareCrc32};

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(9_000),
        2_000,
    )
    .unwrap()
}

fn reversed_calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        2_000,
        Angle::from_centidegrees(9_000),
        1_000,
    )
    .unwrap()
}

// Constructs wire-format fixtures independently of encode().
fn append_entry(
    data: &mut Vec<u8>,
    id: u32,
    lower_angle: i32,
    lower_pulse: u32,
    upper_angle: i32,
    upper_pulse: u32,
) {
    data.extend_from_slice(&id.to_le_bytes());
    data.extend_from_slice(&lower_angle.to_le_bytes());
    data.extend_from_slice(&lower_pulse.to_le_bytes());
    data.extend_from_slice(&upper_angle.to_le_bytes());
    data.extend_from_slice(&upper_pulse.to_le_bytes());
}

fn record(data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0x48, 0x01]);
    bytes.extend_from_slice(
        &u16::try_from(data.len())
            .unwrap()
            .to_le_bytes(),
    );
    bytes.extend_from_slice(data);
    refresh_crc(&mut bytes);
    bytes
}

// Also supports fixtures with deliberately invalid metadata or values.
fn refresh_crc(bytes_without_crc: &mut Vec<u8>) {
    let crc = SoftwareCrc32.checksum(bytes_without_crc);
    bytes_without_crc.extend_from_slice(&crc.to_le_bytes());
}

fn single_record(id: u32) -> Vec<u8> {
    let mut data = Vec::new();
    append_entry(
        &mut data, id, -4_500, 1_000, 9_000, 2_000,
    );
    record(&data)
}

#[test]
fn encode_empty_configuration_writes_header_and_crc() {
    let mut output = [0xAA; 8];

    let length = encode(
        &[None::<Calibration>; 2],
        &mut output,
        &mut SoftwareCrc32,
    )
    .unwrap();

    assert_eq!(length, 8);
    assert_eq!(
        output.as_slice(),
        record(&[]).as_slice()
    );
}

#[test]
fn encode_writes_exact_little_endian_representation() {
    let mut output = [0; 28];

    let length = encode(
        &[Some(calibration())],
        &mut output,
        &mut SoftwareCrc32,
    )
    .unwrap();

    assert_eq!(length, 28);
    assert_eq!(
        output.as_slice(),
        single_record(0).as_slice()
    );
}

#[test]
fn encode_skips_missing_entries_and_preserves_ids() {
    let mut output = [0; 48];

    let length = encode(
        &[Some(calibration()), None, Some(reversed_calibration())],
        &mut output,
        &mut SoftwareCrc32,
    )
    .unwrap();

    let mut data = Vec::new();
    append_entry(
        &mut data, 0, -4_500, 1_000, 9_000, 2_000,
    );
    append_entry(
        &mut data, 2, -4_500, 2_000, 9_000, 1_000,
    );

    assert_eq!(length, 48);
    assert_eq!(
        output.as_slice(),
        record(&data).as_slice()
    );
}

#[test]
fn encode_leaves_unused_output_bytes_unchanged() {
    let mut output = [0xAA; 40];

    let length = encode(
        &[Some(calibration())],
        &mut output,
        &mut SoftwareCrc32,
    )
    .unwrap();

    assert_eq!(length, 28);
    assert_eq!(&output[length..], &[0xAA; 12]);
}

#[test]
fn encode_rejects_insufficient_output_space() {
    let mut output = [0; 27];

    assert_eq!(
        encode(
            &[Some(calibration())],
            &mut output,
            &mut SoftwareCrc32
        ),
        Err(FormatError::BufferTooSmall),
    );
}

#[test]
fn encode_rejects_payload_exceeding_u16_length() {
    // 3277 entries require 65540 payload bytes.
    let calibrations = [Some(calibration()); 3277];
    let mut output = [0; 65_548];

    assert_eq!(
        encode(
            &calibrations,
            &mut output,
            &mut SoftwareCrc32
        ),
        Err(FormatError::TooManyEntries),
    );
}

#[test]
fn decode_empty_configuration_returns_missing_entries() {
    assert_eq!(
        decode::<2>(&record(&[]), &mut SoftwareCrc32),
        Ok([None; 2]),
    );
}

#[test]
fn decode_accepts_unordered_entries_and_reversed_pulses() {
    let mut data = Vec::new();
    append_entry(
        &mut data, 2, -4_500, 2_000, 9_000, 1_000,
    );
    append_entry(
        &mut data, 0, -4_500, 1_000, 9_000, 2_000,
    );

    assert_eq!(
        decode::<3>(&record(&data), &mut SoftwareCrc32),
        Ok([Some(calibration()), None, Some(reversed_calibration())]),
    );
}

#[test]
fn decode_rejects_every_truncated_prefix() {
    let bytes = single_record(0);

    for length in 0..bytes.len() {
        assert_eq!(
            decode::<1>(&bytes[..length], &mut SoftwareCrc32),
            Err(FormatError::InvalidLength),
            "prefix length: {length}",
        );
    }
}

#[test]
fn decode_rejects_unknown_format_identifier() {
    let mut bytes = single_record(0);
    bytes.truncate(bytes.len() - 4);
    bytes[0] = 0x49;
    refresh_crc(&mut bytes);

    assert_eq!(
        decode::<1>(&bytes, &mut SoftwareCrc32),
        Err(FormatError::InvalidMagic),
    );
}

#[test]
fn decode_rejects_unknown_format_version() {
    let mut bytes = single_record(0);
    bytes.truncate(bytes.len() - 4);
    bytes[1] = 2;
    refresh_crc(&mut bytes);

    assert_eq!(
        decode::<1>(&bytes, &mut SoftwareCrc32),
        Err(FormatError::InvalidMagic),
    );
}

#[test]
fn decode_rejects_payload_length_not_divisible_by_twenty() {
    assert_eq!(
        decode::<1>(&record(&[0; 19]), &mut SoftwareCrc32),
        Err(FormatError::InvalidLength),
    );
}

#[test]
fn decode_rejects_trailing_bytes() {
    let mut bytes = single_record(0);
    bytes.push(0);

    assert_eq!(
        decode::<1>(&bytes, &mut SoftwareCrc32),
        Err(FormatError::InvalidLength),
    );
}

#[test]
fn decode_rejects_corrupted_payload() {
    let mut bytes = single_record(0);
    bytes[12] ^= 1;

    assert_eq!(
        decode::<1>(&bytes, &mut SoftwareCrc32),
        Err(FormatError::CrcMismatch),
    );
}

#[test]
fn decode_rejects_corrupted_crc() {
    let mut bytes = single_record(0);
    let last = bytes.len() - 1;
    bytes[last] ^= 1;

    assert_eq!(
        decode::<1>(&bytes, &mut SoftwareCrc32),
        Err(FormatError::CrcMismatch),
    );
}

#[test]
fn decode_rejects_unknown_servo() {
    assert_eq!(
        decode::<2>(&single_record(2), &mut SoftwareCrc32),
        Err(FormatError::UnknownServo(2)),
    );
}

#[test]
fn decode_rejects_duplicate_servo() {
    let mut data = Vec::new();
    append_entry(
        &mut data, 0, -4_500, 1_000, 9_000, 2_000,
    );
    append_entry(
        &mut data, 0, -4_500, 2_000, 9_000, 1_000,
    );

    assert_eq!(
        decode::<2>(&record(&data), &mut SoftwareCrc32),
        Err(FormatError::DuplicateServo(0)),
    );
}

#[test]
fn decode_rejects_invalid_calibration_with_valid_crc() {
    let cases = [
        (
            9_000,
            1_000,
            -4_500,
            2_000,
            CalibrationError::InvalidAngleRange,
        ),
        (
            0,
            1_000,
            0,
            2_000,
            CalibrationError::InvalidAngleRange,
        ),
        (
            -4_500,
            0,
            9_000,
            2_000,
            CalibrationError::ZeroPulseWidth,
        ),
        (
            -4_500,
            1_000,
            9_000,
            0,
            CalibrationError::ZeroPulseWidth,
        ),
        (
            -4_500,
            1_000,
            9_000,
            1_000,
            CalibrationError::EqualPulseWidths,
        ),
    ];

    for (lower, lower_pulse, upper, upper_pulse, error) in cases {
        let mut data = Vec::new();
        append_entry(
            &mut data,
            0,
            lower,
            lower_pulse,
            upper,
            upper_pulse,
        );

        assert_eq!(
            decode::<1>(&record(&data), &mut SoftwareCrc32),
            Err(FormatError::InvalidCalibration { id: 0, error }),
        );
    }
}
