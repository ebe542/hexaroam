use hexaroam_servo_control::{Angle, Calibration};

use super::scan_calibrations;
use crate::{
    CalibrationLoadError, JournalRead, JournalRecord, LocatedRecord, SLOT_SIZE,
    SoftwareCrc32, encode_slot,
};

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

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .unwrap()
}

fn located<const N: usize>(
    slot_index: usize,
    sequence: u64,
    calibrations: [Option<Calibration>; N],
) -> LocatedRecord<N> {
    LocatedRecord {
        slot_index,
        record: JournalRecord {
            sequence,
            calibrations,
        },
    }
}

#[test]
fn no_valid_record_returns_none() {
    for fill in [0xFF, 0] {
        let mut reader = MemoryReader::<4>::new();
        reader.slots = [[fill; SLOT_SIZE]; 4];

        assert_eq!(
            scan_calibrations::<1, 4, _>(&mut reader, &mut SoftwareCrc32,),
            Ok(None)
        );
        assert_eq!(reader.reads, [1; 4]);
    }
}

#[test]
fn returns_slot_sequence_and_calibration() {
    let mut reader = MemoryReader::<4>::new();
    let values = [Some(calibration())];
    reader.seed(2, 15, values);

    assert_eq!(
        scan_calibrations::<1, 4, _>(&mut reader, &mut SoftwareCrc32,),
        Ok(Some(located(2, 15, values)))
    );
}

#[test]
fn highest_sequence_wins_regardless_of_slot_position() {
    for (newest, older) in [(0, 3), (3, 0), (1, 2)] {
        let mut reader = MemoryReader::<4>::new();
        let values = [Some(calibration())];

        reader.seed(older, 10, values);
        reader.seed(newest, 20, values);

        assert_eq!(
            scan_calibrations::<1, 4, _>(&mut reader, &mut SoftwareCrc32,),
            Ok(Some(located(newest, 20, values)))
        );
    }
}

#[test]
fn newest_incomplete_record_is_returned_without_fallback() {
    let mut reader = MemoryReader::<4>::new();
    let complete = [Some(calibration()); 2];
    let incomplete = [Some(calibration()), None];

    reader.seed(0, 10, complete);
    reader.seed(3, 20, incomplete);

    assert_eq!(
        scan_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32,),
        Ok(Some(located(3, 20, incomplete)))
    );
}

#[test]
fn empty_valid_record_retains_its_sequence_and_location() {
    let mut reader = MemoryReader::<4>::new();
    let empty = [None::<Calibration>; 2];

    reader.seed(0, 10, [Some(calibration()); 2]);
    reader.seed(3, 20, empty);

    assert_eq!(
        scan_calibrations::<2, 4, _>(&mut reader, &mut SoftwareCrc32,),
        Ok(Some(located(3, 20, empty)))
    );
}

#[test]
fn corrupt_newer_record_is_skipped() {
    let mut reader = MemoryReader::<4>::new();
    let values = [Some(calibration())];

    reader.seed(0, 10, values);
    reader.seed(3, 20, values);

    // Sequence bytes are covered by the enclosing CRC.
    reader.slots[3][4] ^= 1;

    assert_eq!(
        scan_calibrations::<1, 4, _>(&mut reader, &mut SoftwareCrc32,),
        Ok(Some(located(0, 10, values)))
    );
}

#[test]
fn duplicate_highest_sequence_is_rejected() {
    let mut reader = MemoryReader::<4>::new();
    let values = [Some(calibration())];

    reader.seed(0, 20, values);
    reader.seed(3, 20, values);

    assert_eq!(
        scan_calibrations::<1, 4, _>(&mut reader, &mut SoftwareCrc32,),
        Err(CalibrationLoadError::DuplicateSequence(
            20
        ))
    );
}

#[test]
fn duplicate_older_sequence_is_rejected() {
    let mut reader = MemoryReader::<4>::new();
    let values = [Some(calibration())];

    reader.seed(0, 30, values);
    reader.seed(1, 10, values);
    reader.seed(3, 10, values);

    assert_eq!(
        scan_calibrations::<1, 4, _>(&mut reader, &mut SoftwareCrc32,),
        Err(CalibrationLoadError::DuplicateSequence(
            10
        ))
    );
}

#[test]
fn read_errors_abort_without_returning_a_candidate() {
    for failing_index in [0, 3] {
        let mut reader = MemoryReader::<4>::new();
        reader.seed(0, 10, [Some(calibration())]);
        reader.fail_at = Some(failing_index);

        assert_eq!(
            scan_calibrations::<1, 4, _>(&mut reader, &mut SoftwareCrc32,),
            Err(CalibrationLoadError::Storage(
                ReadError::Failed
            ))
        );

        for index in 0..4 {
            assert_eq!(
                reader.reads[index],
                usize::from(index <= failing_index),
            );
        }
    }
}

#[test]
fn reads_all_64_slots_once_in_ascending_order() {
    let mut reader = MemoryReader::<64>::new();
    let values = [Some(calibration())];
    reader.seed(63, 25, values);

    assert_eq!(
        scan_calibrations::<1, 64, _>(&mut reader, &mut SoftwareCrc32,),
        Ok(Some(located(63, 25, values)))
    );
    assert_eq!(reader.reads, [1; 64]);
}

#[test]
fn single_slot_region_is_supported() {
    let mut reader = MemoryReader::<1>::new();
    let values = [Some(calibration())];
    reader.seed(0, 1, values);

    assert_eq!(
        scan_calibrations::<1, 1, _>(&mut reader, &mut SoftwareCrc32,),
        Ok(Some(located(0, 1, values)))
    );
}

#[test]
fn sequence_extremes_are_returned_without_incrementing() {
    for sequence in [0, u64::MAX] {
        let mut reader = MemoryReader::<1>::new();
        let values = [Some(calibration())];
        reader.seed(0, sequence, values);

        assert_eq!(
            scan_calibrations::<1, 1, _>(&mut reader, &mut SoftwareCrc32,),
            Ok(Some(located(0, sequence, values)))
        );
    }
}

#[test]
fn zero_slots_are_rejected_without_storage_access() {
    let mut reader = MemoryReader::<0>::new();

    assert_eq!(
        scan_calibrations::<1, 0, _>(&mut reader, &mut SoftwareCrc32,),
        Err(CalibrationLoadError::InvalidSlotCount)
    );
    assert_eq!(reader.next_index, 0);
}

#[test]
fn zero_servos_are_rejected_without_storage_access() {
    let mut reader = MemoryReader::<4>::new();

    assert_eq!(
        scan_calibrations::<0, 4, _>(&mut reader, &mut SoftwareCrc32,),
        Err(CalibrationLoadError::InvalidServoCount)
    );
    assert_eq!(reader.reads, [0; 4]);
}
