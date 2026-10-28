use hexaroam_servo_control::{Angle, Calibration};

use super::{JournalRecord, SLOT_SIZE, SlotError, decode_slot, encode_slot};
use crate::{Checksum, FormatError, SoftwareCrc32, encode};

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(9_000),
        2_000,
    )
    .unwrap()
}

// Constructs the journal envelope independently of encode_slot().
fn fixture(sequence: u64) -> [u8; SLOT_SIZE] {
    let mut slot = [0xFF; SLOT_SIZE];
    let mut payload = [0; 28];

    let length = encode(
        &[Some(calibration())],
        &mut payload,
        &mut SoftwareCrc32,
    )
    .unwrap();

    slot[..2].copy_from_slice(&[0x4A, 0x01]);
    slot[2..4].copy_from_slice(&(length as u16).to_le_bytes());
    slot[4..12].copy_from_slice(&sequence.to_le_bytes());
    slot[12..12 + length].copy_from_slice(&payload);

    refresh_journal_crc(&mut slot);
    slot
}

fn refresh_journal_crc(slot: &mut [u8; SLOT_SIZE]) {
    let length = usize::from(u16::from_le_bytes([slot[2], slot[3]]));
    let crc_offset = 12 + length;
    let crc = SoftwareCrc32.checksum(&slot[..crc_offset]);

    slot[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn encode_matches_expected_envelope() {
    let sequence = 0x0102_0304_0506_0708;
    let mut output = [0; SLOT_SIZE];

    encode_slot(
        sequence,
        &[Some(calibration())],
        &mut output,
        &mut SoftwareCrc32,
    )
    .unwrap();

    assert_eq!(output, fixture(sequence));
}

#[test]
fn encode_fills_unused_bytes_with_erased_value() {
    let mut output = [0; SLOT_SIZE];

    encode_slot(
        1,
        &[Some(calibration())],
        &mut output,
        &mut SoftwareCrc32,
    )
    .unwrap();

    // 12-byte header + 28-byte calibration record + 4-byte journal CRC.
    assert!(
        output[44..]
            .iter()
            .all(|&byte| byte == 0xFF)
    );
}

#[test]
fn encode_rejects_payload_exceeding_slot_capacity() {
    // 25 calibrations need 508 bytes before the journal envelope.
    let calibrations = [Some(calibration()); 25];
    let mut output = [0xFF; SLOT_SIZE];

    assert_eq!(
        encode_slot(
            1,
            &calibrations,
            &mut output,
            &mut SoftwareCrc32,
        ),
        Err(SlotError::PayloadTooLarge),
    );
}

#[test]
fn decode_returns_sequence_and_calibrations() {
    let sequence = 0x0102_0304_0506_0708;

    assert_eq!(
        decode_slot::<2>(&fixture(sequence), &mut SoftwareCrc32),
        Ok(JournalRecord {
            sequence,
            calibrations: [Some(calibration()), None],
        }),
    );
}

#[test]
fn erased_slot_is_empty() {
    assert_eq!(
        decode_slot::<1>(&[0xFF; SLOT_SIZE], &mut SoftwareCrc32),
        Err(SlotError::Empty),
    );
}

#[test]
fn decode_rejects_unknown_identifier_and_version() {
    for (offset, value) in [(0, 0x4B), (1, 2)] {
        let mut slot = fixture(1);
        slot[offset] = value;
        refresh_journal_crc(&mut slot);

        assert_eq!(
            decode_slot::<1>(&slot, &mut SoftwareCrc32),
            Err(SlotError::InvalidMagic),
        );
    }
}

#[test]
fn decode_rejects_invalid_payload_lengths() {
    // Minimum inner record: 8 bytes.
    // Maximum payload: 512 - 12 - 4 = 496 bytes.
    for length in [0_u16, 7, 497, u16::MAX] {
        let mut slot = fixture(1);
        slot[2..4].copy_from_slice(&length.to_le_bytes());

        assert_eq!(
            decode_slot::<1>(&slot, &mut SoftwareCrc32),
            Err(SlotError::InvalidLength),
            "payload length: {length}",
        );
    }
}

#[test]
fn sequence_is_protected_by_journal_crc() {
    let mut slot = fixture(1);
    slot[4] ^= 1;

    assert_eq!(
        decode_slot::<1>(&slot, &mut SoftwareCrc32),
        Err(SlotError::CrcMismatch),
    );
}

#[test]
fn payload_is_protected_by_journal_crc() {
    let mut slot = fixture(1);
    slot[24] ^= 1;

    assert_eq!(
        decode_slot::<1>(&slot, &mut SoftwareCrc32),
        Err(SlotError::CrcMismatch),
    );
}

#[test]
fn decode_rejects_corrupted_journal_crc() {
    let mut slot = fixture(1);
    slot[40] ^= 1;

    assert_eq!(
        decode_slot::<1>(&slot, &mut SoftwareCrc32),
        Err(SlotError::CrcMismatch),
    );
}

#[test]
fn decode_validates_inner_record_despite_valid_journal_crc() {
    let mut slot = fixture(1);

    // Damage an inner pulse value, then repair only the outer CRC.
    slot[24] ^= 1;
    refresh_journal_crc(&mut slot);

    assert_eq!(
        decode_slot::<1>(&slot, &mut SoftwareCrc32),
        Err(SlotError::Format(
            FormatError::CrcMismatch
        )),
    );
}

#[test]
fn decode_ignores_padding() {
    let mut slot = fixture(1);
    slot[44..].fill(0);

    assert_eq!(
        decode_slot::<1>(&slot, &mut SoftwareCrc32),
        Ok(JournalRecord {
            sequence: 1,
            calibrations: [Some(calibration())],
        }),
    );
}

#[test]
fn maximum_sequence_number_can_be_loaded() {
    assert_eq!(
        decode_slot::<1>(&fixture(u64::MAX), &mut SoftwareCrc32),
        Ok(JournalRecord {
            sequence: u64::MAX,
            calibrations: [Some(calibration())],
        }),
    );
}
