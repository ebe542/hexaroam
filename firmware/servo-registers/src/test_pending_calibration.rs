extern crate std;

use self::std::vec::Vec;

use super::*;
use hexaroam_servo_control::{Angle, CalibrationError};
use hexaroam_servo_storage::SoftwareCrc32;

type Entry = (u32, i32, u32, i32, u32);

fn entry(id: u32) -> Entry {
    (id, -4_500, 1_000, 4_500, 2_000)
}

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .unwrap()
}

// Construct fixtures independently of the production encoder.
fn record(entries: &[Entry]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0x48, 0x01]);

    let length = u16::try_from(entries.len() * 20).unwrap();
    bytes.extend_from_slice(&length.to_le_bytes());

    for &(id, lower, lower_pulse, upper, upper_pulse) in entries {
        bytes.extend_from_slice(&id.to_le_bytes());
        bytes.extend_from_slice(&lower.to_le_bytes());
        bytes.extend_from_slice(&lower_pulse.to_le_bytes());
        bytes.extend_from_slice(&upper.to_le_bytes());
        bytes.extend_from_slice(&upper_pulse.to_le_bytes());
    }

    let crc = SoftwareCrc32.checksum(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes
}

fn refresh_crc(bytes: &mut [u8]) {
    let offset = bytes.len() - 4;
    let crc = SoftwareCrc32.checksum(&bytes[..offset]);
    bytes[offset..].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn complete_set_preserves_individual_calibrations() {
    let reversed = Calibration::new(
        Angle::from_centidegrees(-9_000),
        2_400,
        Angle::from_centidegrees(9_000),
        600,
    )
    .unwrap();

    let bytes = record(&[entry(0), (1, -9_000, 2_400, 9_000, 600)]);

    let pending =
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32)
            .unwrap();

    assert_eq!(
        pending.calibrations(),
        &[Some(calibration()), Some(reversed)],
    );
}

#[test]
fn unordered_entries_are_assigned_by_servo_id() {
    let bytes = record(&[entry(1), entry(0)]);

    let pending =
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32)
            .unwrap();

    assert_eq!(
        pending.calibrations(),
        &[Some(calibration()); 2],
    );
}

#[test]
fn pending_data_outlives_the_receive_buffer() {
    let pending = {
        let mut bytes = record(&[entry(0), entry(1)]);

        let pending =
            PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32)
                .unwrap();

        // Overwriting and dropping the input must not alter the request.
        bytes.fill(0);
        pending
    };

    assert_eq!(
        pending.calibrations(),
        &[Some(calibration()); 2],
    );
}

#[test]
fn every_missing_servo_is_rejected() {
    for missing in 0..3 {
        let entries: Vec<Entry> = (0..3)
            .filter(|&id| id != missing)
            .map(entry)
            .collect();
        let bytes = record(&entries);

        assert_eq!(
            PendingCalibration::<3>::from_bytes(&bytes, &mut SoftwareCrc32,),
            Err(PendingCalibrationError::MissingServo(
                ServoId(missing as usize),
            )),
        );
    }
}

#[test]
fn empty_set_reports_first_missing_servo() {
    let bytes = record(&[]);

    assert_eq!(
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32),
        Err(PendingCalibrationError::MissingServo(
            ServoId(0)
        )),
    );
}

#[test]
fn duplicate_servo_error_is_preserved() {
    let bytes = record(&[entry(0), entry(0)]);

    assert_eq!(
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32),
        Err(PendingCalibrationError::Format(
            FormatError::DuplicateServo(0),
        )),
    );
}

#[test]
fn unknown_servo_error_is_preserved() {
    let bytes = record(&[entry(0), entry(2)]);

    assert_eq!(
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32),
        Err(PendingCalibrationError::Format(
            FormatError::UnknownServo(2),
        )),
    );
}

#[test]
fn invalid_magic_is_rejected_even_with_valid_crc() {
    let mut bytes = record(&[entry(0), entry(1)]);
    bytes[0] ^= 1;
    refresh_crc(&mut bytes);

    assert_eq!(
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32),
        Err(PendingCalibrationError::Format(
            FormatError::InvalidMagic,
        )),
    );
}

#[test]
fn truncated_and_trailing_data_are_rejected() {
    let bytes = record(&[entry(0), entry(1)]);

    for length in 0..bytes.len() {
        assert_eq!(
            PendingCalibration::<2>::from_bytes(
                &bytes[..length],
                &mut SoftwareCrc32,
            ),
            Err(PendingCalibrationError::Format(
                FormatError::InvalidLength,
            )),
        );
    }

    let mut extra = bytes;
    extra.push(0);

    assert_eq!(
        PendingCalibration::<2>::from_bytes(&extra, &mut SoftwareCrc32),
        Err(PendingCalibrationError::Format(
            FormatError::InvalidLength,
        )),
    );
}

#[test]
fn damaged_crc_is_rejected() {
    let mut bytes = record(&[entry(0), entry(1)]);
    let last = bytes.len() - 1;
    bytes[last] ^= 1;

    assert_eq!(
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32),
        Err(PendingCalibrationError::Format(
            FormatError::CrcMismatch,
        )),
    );
}

#[test]
fn invalid_calibration_error_is_preserved() {
    let bytes = record(&[entry(0), (1, 4_500, 1_000, -4_500, 2_000)]);

    assert_eq!(
        PendingCalibration::<2>::from_bytes(&bytes, &mut SoftwareCrc32),
        Err(PendingCalibrationError::Format(
            FormatError::InvalidCalibration {
                id: 1,
                error: CalibrationError::InvalidAngleRange,
            },
        )),
    );
}

#[test]
fn complete_eighteen_servo_set_is_accepted() {
    let entries: Vec<Entry> = (0..18).map(entry).collect();
    let bytes = record(&entries);

    let pending =
        PendingCalibration::<18>::from_bytes(&bytes, &mut SoftwareCrc32)
            .unwrap();

    assert_eq!(
        pending.calibrations(),
        &[Some(calibration()); 18],
    );
}
