use hexaroam_servo_control::{Angle, Calibration};

use super::{CalibrationLoadError, load_calibrations};
use crate::{JournalRead, SLOT_SIZE, SoftwareCrc32, encode_slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadError {
    Failed,
}

struct MemoryReader<const SLOTS: usize> {
    slots: [[u8; SLOT_SIZE]; SLOTS],
    reads: [usize; SLOTS],
    next_index: usize,
    fail_at: Option<usize>,
}

impl<const SLOTS: usize> MemoryReader<SLOTS> {
    fn new() -> Self {
        Self {
            slots: [[0xFF; SLOT_SIZE]; SLOTS],
            reads: [0; SLOTS],
            next_index: 0,
            fail_at: None,
        }
    }

    fn seed<const N: usize>(
        &mut self,
        index: usize,
        sequence: u64,
        calibrations: [Option<Calibration>; N],
    ) {
        encode_slot(
            sequence,
            &calibrations,
            &mut self.slots[index],
            &mut SoftwareCrc32,
        )
        .unwrap();
    }
}

impl<const SLOTS: usize> JournalRead for MemoryReader<SLOTS> {
    type Error = ReadError;

    fn read_slot(
        &mut self,
        index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        assert_eq!(index, self.next_index);
        self.next_index += 1;
        self.reads[index] += 1;

        if self.fail_at == Some(index) {
            return Err(ReadError::Failed);
        }

        output.copy_from_slice(&self.slots[index]);
        Ok(())
    }
}

fn calibration(lower_pulse: u32) -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        lower_pulse,
        Angle::from_centidegrees(4_500),
        lower_pulse + 1_000,
    )
    .unwrap()
}

fn complete() -> [Option<Calibration>; 2] {
    [Some(calibration(1_000)), Some(calibration(1_200))]
}

fn expected() -> [Calibration; 2] {
    [calibration(1_000), calibration(1_200)]
}

#[test]
fn no_valid_record_returns_none() {
    for fill in [0xFF, 0] {
        let mut reader = MemoryReader::<4>::new();
        reader.slots = [[fill; SLOT_SIZE]; 4];

        assert_eq!(
            load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32,),
            Ok(None)
        );
        assert_eq!(reader.reads, [1; 4]);
    }
}

#[test]
fn loads_complete_calibration_in_servo_id_order() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(2, 10, complete());

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
}

#[test]
fn highest_sequence_wins_regardless_of_slot_position() {
    for (newest, older) in [(0, 3), (3, 0), (1, 2)] {
        let mut reader = MemoryReader::<4>::new();
        reader.seed(older, 10, [Some(calibration(800)); 2]);
        reader.seed(newest, 20, complete());

        assert_eq!(
            load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32,),
            Ok(Some(expected()))
        );
    }
}

#[test]
fn scans_all_64_eeprom_slots_exactly_once() {
    let mut reader = MemoryReader::<64>::new();
    reader.seed(63, 30, complete());

    assert_eq!(
        load_calibrations::<2, 64, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
    assert_eq!(reader.reads, [1; 64]);
}

#[test]
fn sequence_zero_is_a_valid_sequence() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 0, complete());

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
}

#[test]
fn corrupt_newer_record_is_skipped() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 10, complete());
    reader.seed(3, 20, [Some(calibration(800)); 2]);

    // Change the sequence without updating the enclosing CRC.
    reader.slots[3][4] ^= 1;

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
}

#[test]
fn record_with_invalid_magic_is_skipped() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 10, complete());
    reader.seed(3, 20, [Some(calibration(800)); 2]);
    reader.slots[3][0] ^= 1;

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
}

#[test]
fn duplicate_highest_sequence_is_rejected_even_for_identical_data() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 20, complete());
    reader.seed(3, 20, complete());

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::DuplicateSequence(
            20
        ))
    );
}

#[test]
fn duplicate_older_sequence_is_also_rejected() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 30, complete());
    reader.seed(1, 10, complete());
    reader.seed(3, 10, complete());

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::DuplicateSequence(
            10
        ))
    );
}

#[test]
fn corrupt_duplicate_does_not_count_as_a_valid_duplicate() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 20, complete());
    reader.seed(3, 20, complete());
    reader.slots[3][0] ^= 1;

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
}

#[test]
fn read_error_at_first_slot_aborts_loading() {
    let mut reader = MemoryReader::<4>::new();
    reader.fail_at = Some(0);

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::Storage(
            ReadError::Failed
        ))
    );
    assert_eq!(reader.reads, [1, 0, 0, 0]);
}

#[test]
fn later_read_error_prevents_returning_an_earlier_record() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 10, complete());
    reader.fail_at = Some(3);

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::Storage(
            ReadError::Failed
        ))
    );
    assert_eq!(reader.reads, [1; 4]);
}

#[test]
fn incomplete_newest_record_prevents_fallback() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 10, complete());
    reader.seed(3, 20, [Some(calibration(800)), None]);

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::IncompleteCalibration(1))
    );
}

#[test]
fn missing_first_servo_is_reported() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(2, 20, [None, Some(calibration(800))]);

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::IncompleteCalibration(0))
    );
}

#[test]
fn empty_valid_record_is_incomplete_instead_of_absent() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(2, 20, [None::<Calibration>; 2]);

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::IncompleteCalibration(0))
    );
}

#[test]
fn older_incomplete_record_does_not_block_newer_complete_record() {
    let mut reader = MemoryReader::<4>::new();
    reader.seed(0, 10, [Some(calibration(800)), None]);
    reader.seed(3, 20, complete());

    assert_eq!(
        load_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
}

#[test]
fn single_slot_storage_is_supported() {
    let mut reader = MemoryReader::<1>::new();
    reader.seed(0, 10, complete());

    assert_eq!(
        load_calibrations::<2, 1, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(expected()))
    );
    assert_eq!(reader.reads, [1]);
}

#[test]
fn zero_slots_are_rejected_without_storage_access() {
    let mut reader = MemoryReader::<0>::new();

    assert_eq!(
        load_calibrations::<2, 0, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::InvalidSlotCount)
    );
    assert_eq!(reader.next_index, 0);
}

#[test]
fn zero_servos_are_rejected_without_storage_access() {
    let mut reader = MemoryReader::<4>::new();

    assert_eq!(
        load_calibrations::<0, 4, _>(&mut reader, &mut SoftwareCrc32),
        Err(CalibrationLoadError::InvalidServoCount)
    );
    assert_eq!(reader.reads, [0; 4]);
}
