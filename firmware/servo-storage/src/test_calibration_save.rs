extern crate std;

use std::vec::Vec;

use hexaroam_servo_control::{Angle, Calibration};

use crate::{
    CalibrationLoadError, CalibrationSaveError, CalibrationWritePlanError,
    JournalRead, SLOT_SIZE, SlotError, SlotWrite, SlotWriteError,
    SoftwareCrc32, decode_slot, encode_slot, save_calibrations,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Failed,
}

#[derive(Clone, Copy)]
enum Tamper {
    None,
    Body,
    Padding,
}

struct WriteEvent {
    slot: usize,
    offset: usize,
    data: Vec<u8>,
}

struct MemoryStorage<const SLOTS: usize> {
    slots: [[u8; SLOT_SIZE]; SLOTS],
    reads: usize,
    writes: Vec<WriteEvent>,
    fail_read: Option<usize>,
    fail_write: Option<usize>,
    ignore_invalidation: [bool; SLOTS],
    tamper: [Tamper; SLOTS],
}

impl<const SLOTS: usize> MemoryStorage<SLOTS> {
    fn new() -> Self {
        Self {
            slots: [[0xFF; SLOT_SIZE]; SLOTS],
            reads: 0,
            writes: Vec::new(),
            fail_read: None,
            fail_write: None,
            ignore_invalidation: [false; SLOTS],
            tamper: [Tamper::None; SLOTS],
        }
    }

    fn seed(
        &mut self,
        index: usize,
        sequence: u64,
        value: Option<Calibration>,
    ) {
        encode_slot(
            sequence,
            &[value],
            &mut self.slots[index],
            &mut SoftwareCrc32,
        )
        .unwrap();
    }

    fn attempted_slots(&self) -> Vec<usize> {
        self.writes
            .iter()
            .filter(|event| {
                event.offset == 0 && event.data.as_slice() == [0, 0]
            })
            .map(|event| event.slot)
            .collect()
    }
}

impl<const SLOTS: usize> JournalRead for MemoryStorage<SLOTS> {
    type Error = TestError;

    fn read_slot(
        &mut self,
        index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        self.reads += 1;

        if self.fail_read == Some(self.reads) {
            return Err(TestError::Failed);
        }

        output.copy_from_slice(&self.slots[index]);
        Ok(())
    }
}

impl<const SLOTS: usize> SlotWrite for MemoryStorage<SLOTS> {
    fn write_slot_bytes(
        &mut self,
        index: usize,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        assert!(index < SLOTS);
        assert!(offset <= SLOT_SIZE);
        assert!(data.len() <= SLOT_SIZE - offset);

        self.writes.push(WriteEvent {
            slot: index,
            offset,
            data: data.to_vec(),
        });

        if self.fail_write == Some(self.writes.len()) {
            // A failed operation may already have changed a byte.
            if !data.is_empty() {
                self.slots[index][offset] = 0xEE;
            }
            return Err(TestError::Failed);
        }

        if offset == 0 && data == [0, 0] && self.ignore_invalidation[index] {
            return Ok(());
        }

        self.slots[index][offset..offset + data.len()].copy_from_slice(data);

        if offset == 0 && data == [0x4A, 0x01] {
            match self.tamper[index] {
                Tamper::None => {}
                Tamper::Body => self.slots[index][20] ^= 1,
                Tamper::Padding => {
                    self.slots[index][SLOT_SIZE - 1] ^= 1;
                }
            }
        }

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

fn save<const SLOTS: usize>(
    storage: &mut MemoryStorage<SLOTS>,
) -> Result<(), CalibrationSaveError<TestError>> {
    save_calibrations::<1, SLOTS, _>(
        storage,
        &[calibration(1_100)],
        &mut SoftwareCrc32,
    )
}

fn assert_saved<const SLOTS: usize>(
    storage: &MemoryStorage<SLOTS>,
    index: usize,
    sequence: u64,
) {
    let record = decode_slot::<1>(
        &storage.slots[index],
        &mut SoftwareCrc32,
    )
    .unwrap();

    assert_eq!(record.sequence, sequence);
    assert_eq!(
        record.calibrations,
        [Some(calibration(1_100))]
    );
}

#[test]
fn first_dataset_is_written_to_slot_zero() {
    let mut storage = MemoryStorage::<4>::new();

    assert_eq!(save(&mut storage), Ok(()));
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[0]
    );
    assert_saved(&storage, 0, 1);
}

#[test]
fn changed_dataset_preserves_previous_record() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(1, 10, Some(calibration(1_000)));
    let previous = storage.slots[1];

    assert_eq!(save(&mut storage), Ok(()));
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[2]
    );
    assert_eq!(storage.slots[1], previous);
    assert_saved(&storage, 2, 11);
}

#[test]
fn rotation_wraps_from_last_slot_to_first() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(3, 10, Some(calibration(1_000)));
    let previous = storage.slots[3];

    assert_eq!(save(&mut storage), Ok(()));
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[0]
    );
    assert_eq!(storage.slots[3], previous);
    assert_saved(&storage, 0, 11);
}

#[test]
fn unchanged_data_causes_no_write() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(2, 10, Some(calibration(1_100)));
    let previous = storage.slots;

    assert_eq!(save(&mut storage), Ok(()));
    assert!(storage.writes.is_empty());
    assert_eq!(storage.slots, previous);
    assert_eq!(storage.reads, 4);
}

#[test]
fn incomplete_newest_record_can_be_replaced() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, 10, None);

    assert_eq!(save(&mut storage), Ok(()));
    assert_saved(&storage, 1, 11);
}

#[test]
fn fewer_than_two_slots_are_rejected_before_reading() {
    let mut empty = MemoryStorage::<0>::new();
    let mut single = MemoryStorage::<1>::new();

    for result in [save(&mut empty), save(&mut single)] {
        assert_eq!(
            result,
            Err(CalibrationSaveError::Plan(
                CalibrationWritePlanError::InvalidSlotCount
            ))
        );
    }

    assert_eq!(empty.reads, 0);
    assert_eq!(single.reads, 0);
    assert!(empty.writes.is_empty());
    assert!(single.writes.is_empty());
}

#[test]
fn zero_servos_are_rejected_before_reading() {
    let mut storage = MemoryStorage::<4>::new();

    assert_eq!(
        save_calibrations::<0, 4, _>(&mut storage, &[], &mut SoftwareCrc32,),
        Err(CalibrationSaveError::Plan(
            CalibrationWritePlanError::InvalidServoCount
        ))
    );
    assert_eq!(storage.reads, 0);
    assert!(storage.writes.is_empty());
}

#[test]
fn sequence_overflow_prevents_writing() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, u64::MAX, Some(calibration(1_000)));

    assert_eq!(
        save(&mut storage),
        Err(CalibrationSaveError::Plan(
            CalibrationWritePlanError::SequenceOverflow
        ))
    );
    assert!(storage.writes.is_empty());
}

#[test]
fn unchanged_data_at_maximum_sequence_still_succeeds() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, u64::MAX, Some(calibration(1_100)));

    assert_eq!(save(&mut storage), Ok(()));
    assert!(storage.writes.is_empty());
}

#[test]
fn oversized_dataset_is_rejected_before_writing() {
    let mut storage = MemoryStorage::<4>::new();

    assert_eq!(
        save_calibrations::<25, 4, _>(
            &mut storage,
            &[calibration(1_100); 25],
            &mut SoftwareCrc32,
        ),
        Err(CalibrationSaveError::Write(
            SlotWriteError::Slot(SlotError::PayloadTooLarge)
        ))
    );
    assert!(storage.writes.is_empty());
}

#[test]
fn duplicate_sequences_prevent_writing() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, 10, Some(calibration(1_000)));
    storage.seed(3, 10, Some(calibration(1_000)));

    assert_eq!(
        save(&mut storage),
        Err(CalibrationSaveError::Scan(
            CalibrationLoadError::DuplicateSequence(10)
        ))
    );
    assert!(storage.writes.is_empty());
}

#[test]
fn initial_scan_error_prevents_writing() {
    let mut storage = MemoryStorage::<4>::new();
    storage.fail_read = Some(2);

    assert_eq!(
        save(&mut storage),
        Err(CalibrationSaveError::Scan(
            CalibrationLoadError::Storage(TestError::Failed)
        ))
    );
    assert!(storage.writes.is_empty());
}

#[test]
fn any_write_error_aborts_without_trying_another_slot() {
    for failing_write in [1, 2, 3] {
        let mut storage = MemoryStorage::<4>::new();
        storage.seed(0, 10, Some(calibration(1_000)));
        let previous = storage.slots[0];
        storage.fail_write = Some(failing_write);

        assert_eq!(
            save(&mut storage),
            Err(CalibrationSaveError::Write(
                SlotWriteError::Storage(TestError::Failed)
            ))
        );
        assert_eq!(
            storage.attempted_slots().as_slice(),
            &[1]
        );
        assert_eq!(storage.writes.len(), failing_write);
        assert_eq!(storage.slots[0], previous);
    }
}

#[test]
fn either_verification_read_error_aborts_without_retry() {
    for failing_read in [5, 6] {
        let mut storage = MemoryStorage::<4>::new();
        storage.seed(0, 10, Some(calibration(1_000)));
        storage.fail_read = Some(failing_read);

        assert_eq!(
            save(&mut storage),
            Err(CalibrationSaveError::Write(
                SlotWriteError::Storage(TestError::Failed)
            ))
        );
        assert_eq!(
            storage.attempted_slots().as_slice(),
            &[1]
        );
    }
}

#[test]
fn corrupt_readback_rotates_and_increments_attempt_sequence() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, 10, Some(calibration(1_000)));
    let previous = storage.slots[0];
    storage.tamper[1] = Tamper::Body;

    assert_eq!(save(&mut storage), Ok(()));
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[1, 2]
    );
    assert_eq!(storage.slots[0], previous);
    assert_saved(&storage, 2, 12);
}

#[test]
fn failed_invalidation_rotates_without_overwriting_the_target_body() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, 10, Some(calibration(1_000)));
    storage.seed(1, 5, Some(calibration(900)));
    let untouched = storage.slots[1];
    storage.ignore_invalidation[1] = true;

    assert_eq!(save(&mut storage), Ok(()));
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[1, 2]
    );
    assert_eq!(storage.slots[1], untouched);
    assert_saved(&storage, 2, 12);
}

#[test]
fn valid_but_unverified_record_does_not_short_circuit_retry() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, 10, Some(calibration(1_000)));
    storage.tamper[1] = Tamper::Padding;

    assert_eq!(save(&mut storage), Ok(()));
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[1, 2]
    );

    // Padding is not covered by decoding, but exact write verification
    // rejected this candidate. A fresh attempt still had to succeed.
    assert_saved(&storage, 1, 11);
    assert_saved(&storage, 2, 12);
}

#[test]
fn three_failed_targets_exhaust_attempts_and_preserve_original() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, 10, Some(calibration(1_000)));
    let previous = storage.slots[0];
    storage.ignore_invalidation = [false, true, true, true];

    assert_eq!(
        save(&mut storage),
        Err(
            CalibrationSaveError::AttemptsExhausted {
                attempts: 3,
                last_error: SlotWriteError::InvalidationFailed,
            }
        )
    );
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[1, 2, 3]
    );
    assert_eq!(storage.slots[0], previous);
}

#[test]
fn two_slots_allow_only_one_attempt_when_original_is_protected() {
    let mut storage = MemoryStorage::<2>::new();
    storage.seed(0, 10, Some(calibration(1_000)));
    let previous = storage.slots[0];
    storage.ignore_invalidation[1] = true;

    assert_eq!(
        save(&mut storage),
        Err(
            CalibrationSaveError::AttemptsExhausted {
                attempts: 1,
                last_error: SlotWriteError::InvalidationFailed,
            }
        )
    );
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[1]
    );
    assert_eq!(storage.slots[0], previous);
}

#[test]
fn rescan_error_aborts_before_another_attempt() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(0, 10, Some(calibration(1_000)));
    storage.tamper[1] = Tamper::Body;

    // Four scan reads, two write-verification reads, then rescan.
    storage.fail_read = Some(7);

    assert_eq!(
        save(&mut storage),
        Err(CalibrationSaveError::Scan(
            CalibrationLoadError::Storage(TestError::Failed)
        ))
    );
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[1]
    );
}

#[test]
fn retry_sequence_cannot_wrap_even_if_failed_record_is_invalid() {
    let mut storage = MemoryStorage::<4>::new();
    storage.seed(
        0,
        u64::MAX - 1,
        Some(calibration(1_000)),
    );
    storage.tamper[1] = Tamper::Body;

    assert_eq!(
        save(&mut storage),
        Err(CalibrationSaveError::Plan(
            CalibrationWritePlanError::SequenceOverflow
        ))
    );
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[1]
    );
}

#[test]
fn empty_storage_still_limits_attempts_to_three_targets() {
    let mut storage = MemoryStorage::<4>::new();
    storage.ignore_invalidation = [true; 4];

    assert_eq!(
        save(&mut storage),
        Err(
            CalibrationSaveError::AttemptsExhausted {
                attempts: 3,
                last_error: SlotWriteError::InvalidationFailed,
            }
        )
    );
    assert_eq!(
        storage.attempted_slots().as_slice(),
        &[0, 1, 2]
    );
}
