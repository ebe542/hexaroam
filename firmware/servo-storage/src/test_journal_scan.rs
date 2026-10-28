use hexaroam_servo_control::{Angle, Calibration};

use super::{LocatedRecord, ScanError, find_latest};
use crate::{JournalRecord, SLOT_SIZE, SoftwareCrc32, encode_slot};

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(9_000),
        2_000,
    )
    .unwrap()
}

fn slot<const N: usize>(
    sequence: u64,
    calibrations: [Option<Calibration>; N],
) -> [u8; SLOT_SIZE] {
    let mut bytes = [0xFF; SLOT_SIZE];

    encode_slot(
        sequence,
        &calibrations,
        &mut bytes,
        &mut SoftwareCrc32,
    )
    .unwrap();

    bytes
}

fn complete_slot(sequence: u64) -> [u8; SLOT_SIZE] {
    slot(sequence, [Some(calibration())])
}

fn expected<const N: usize>(
    slot_index: usize,
    sequence: u64,
    calibrations: [Option<Calibration>; N],
) -> Result<Option<LocatedRecord<N>>, ScanError> {
    Ok(Some(LocatedRecord {
        slot_index,
        record: JournalRecord {
            sequence,
            calibrations,
        },
    }))
}

#[test]
fn no_slots_returns_none() {
    assert_eq!(
        find_latest::<1>(&[], &mut SoftwareCrc32),
        Ok(None),
    );
}

#[test]
fn erased_slots_return_none() {
    let slots = [[0xFF; SLOT_SIZE]; 16];

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        Ok(None),
    );
}

#[test]
fn invalid_slots_return_none() {
    let slots = [[0; SLOT_SIZE]; 16];

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        Ok(None),
    );
}

#[test]
fn single_record_with_sequence_zero_is_found() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[5] = complete_slot(0);

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        expected(5, 0, [Some(calibration())]),
    );
}

#[test]
fn highest_sequence_wins_regardless_of_sector_or_address() {
    // Slots 0..7 and 8..15 represent the two sectors.
    for (newest_index, other_index) in [(0, 15), (15, 0), (8, 7), (7, 8)] {
        let mut slots = [[0xFF; SLOT_SIZE]; 16];
        slots[newest_index] = complete_slot(30);
        slots[other_index] = complete_slot(10);

        assert_eq!(
            find_latest::<1>(&slots, &mut SoftwareCrc32),
            expected(newest_index, 30, [Some(calibration())]),
            "newest slot: {newest_index}",
        );
    }
}

#[test]
fn duplicate_highest_sequence_is_rejected() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[2] = complete_slot(20);
    slots[13] = complete_slot(20);

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        Err(ScanError::DuplicateSequence(20)),
    );
}

#[test]
fn duplicate_older_sequence_is_also_rejected() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[0] = complete_slot(30);
    slots[4] = complete_slot(10);
    slots[12] = complete_slot(10);

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        Err(ScanError::DuplicateSequence(10)),
    );
}

#[test]
fn invalid_record_does_not_create_duplicate_sequence() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[2] = complete_slot(20);
    slots[13] = complete_slot(20);

    // Damage the journal identifier.
    slots[13][0] ^= 1;

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        expected(2, 20, [Some(calibration())]),
    );
}

#[test]
fn newer_partial_configuration_replaces_complete_configuration() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[0] = slot(
        10,
        [Some(calibration()), Some(calibration())],
    );
    slots[8] = slot(11, [Some(calibration()), None]);

    assert_eq!(
        find_latest::<2>(&slots, &mut SoftwareCrc32),
        expected(8, 11, [Some(calibration()), None]),
    );
}

#[test]
fn newer_empty_configuration_preserves_invalidation() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[0] = complete_slot(10);
    slots[8] = slot(11, [None::<Calibration>]);

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        expected(8, 11, [None]),
    );
}

#[test]
fn maximum_sequence_is_selected_without_incrementing_it() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[0] = complete_slot(u64::MAX - 1);
    slots[8] = complete_slot(u64::MAX);

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        expected(8, u64::MAX, [Some(calibration())]),
    );
}

#[test]
fn interrupted_new_record_preserves_previous_valid_record() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[0] = complete_slot(10);

    let newer = complete_slot(11);

    // Simulate an interrupted write containing only part of the header.
    slots[8][..10].copy_from_slice(&newer[..10]);

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        expected(0, 10, [Some(calibration())]),
    );
}

#[test]
fn corrupted_sequence_is_not_used_for_selection() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[0] = complete_slot(10);
    slots[8] = complete_slot(11);

    // Change the most significant sequence byte without updating CRC.
    slots[8][11] ^= 0x80;

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        expected(0, 10, [Some(calibration())]),
    );
}

#[test]
fn invalid_inner_configuration_is_skipped() {
    let mut slots = [[0xFF; SLOT_SIZE]; 16];
    slots[0] = complete_slot(10);

    // Both CRCs are valid, but ID1 is unknown to a one-servo reader.
    slots[8] = slot(11, [None, Some(calibration())]);

    assert_eq!(
        find_latest::<1>(&slots, &mut SoftwareCrc32),
        expected(0, 10, [Some(calibration())]),
    );
}
