extern crate std;

use std::{collections::VecDeque, vec, vec::Vec};

use hexaroam_servo_control::{Angle, Calibration};

use crate::{
    JournalRead, SLOT_SIZE, SlotError, SlotWrite, SlotWriteError,
    SoftwareCrc32, encode_slot, write_calibration_slot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Failed,
}

enum Step {
    Write {
        offset: usize,
        data: Vec<u8>,
        result: Result<(), TestError>,
    },
    Read {
        data: [u8; SLOT_SIZE],
        result: Result<(), TestError>,
    },
}

struct TestStorage {
    steps: VecDeque<Step>,
}

impl TestStorage {
    fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: VecDeque::from(steps),
        }
    }

    fn finished(&self) {
        assert!(
            self.steps.is_empty(),
            "Missing expected operations"
        );
    }
}

impl JournalRead for TestStorage {
    type Error = TestError;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        assert_eq!(slot_index, 2);

        match self
            .steps
            .pop_front()
            .expect("Unexpected read")
        {
            Step::Read { data, result } => {
                *output = data;
                result
            }
            Step::Write { .. } => {
                panic!("Expected a write before this read");
            }
        }
    }
}

impl SlotWrite for TestStorage {
    fn write_slot_bytes(
        &mut self,
        slot_index: usize,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        assert_eq!(slot_index, 2);

        match self
            .steps
            .pop_front()
            .expect("Unexpected write")
        {
            Step::Write {
                offset: expected_offset,
                data: expected_data,
                result,
            } => {
                assert_eq!(offset, expected_offset);
                assert_eq!(data, expected_data.as_slice());
                result
            }
            Step::Read { .. } => {
                panic!("Expected a read before this write");
            }
        }
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

fn candidate() -> [u8; SLOT_SIZE] {
    let mut bytes = [0; SLOT_SIZE];

    encode_slot(
        25,
        &[Some(calibration()); 2],
        &mut bytes,
        &mut SoftwareCrc32,
    )
    .unwrap();

    bytes
}

fn write(offset: usize, data: &[u8], result: Result<(), TestError>) -> Step {
    Step::Write {
        offset,
        data: data.to_vec(),
        result,
    }
}

fn read(data: [u8; SLOT_SIZE], result: Result<(), TestError>) -> Step {
    Step::Read { data, result }
}

fn invalidated() -> [u8; SLOT_SIZE] {
    // Old contents need not otherwise match the new candidate.
    let mut bytes = [0xCC; SLOT_SIZE];
    bytes[..2].copy_from_slice(&[0, 0]);
    bytes
}

fn success_steps() -> Vec<Step> {
    let candidate = candidate();

    vec![
        write(0, &[0, 0], Ok(())),
        read(invalidated(), Ok(())),
        write(2, &candidate[2..], Ok(())),
        write(0, &candidate[..2], Ok(())),
        read(candidate, Ok(())),
    ]
}

fn run(steps: Vec<Step>) -> Result<(), SlotWriteError<TestError>> {
    let mut storage = TestStorage::new(steps);

    let result = write_calibration_slot(
        &mut storage,
        2,
        25,
        &[calibration(); 2],
        &mut SoftwareCrc32,
    );

    storage.finished();
    result
}

#[test]
fn writes_body_before_restoring_magic_and_verifies_entire_slot() {
    assert_eq!(run(success_steps()), Ok(()));
}

#[test]
fn zero_servos_are_rejected_before_storage_access() {
    let mut storage = TestStorage::new(vec![]);

    assert_eq!(
        write_calibration_slot::<0, _>(
            &mut storage,
            2,
            25,
            &[],
            &mut SoftwareCrc32,
        ),
        Err(SlotWriteError::InvalidServoCount)
    );
    storage.finished();
}

#[test]
fn oversized_dataset_is_rejected_before_storage_access() {
    let mut storage = TestStorage::new(vec![]);

    assert_eq!(
        write_calibration_slot(
            &mut storage,
            2,
            25,
            &[calibration(); 25],
            &mut SoftwareCrc32,
        ),
        Err(SlotWriteError::Slot(
            SlotError::PayloadTooLarge
        ))
    );
    storage.finished();
}

#[test]
fn invalidation_write_error_aborts_without_retry() {
    assert_eq!(
        run(vec![write(
            0,
            &[0, 0],
            Err(TestError::Failed)
        ),]),
        Err(SlotWriteError::Storage(
            TestError::Failed
        ))
    );
}

#[test]
fn invalidation_read_error_aborts_without_using_returned_bytes() {
    assert_eq!(
        run(vec![
            write(0, &[0, 0], Ok(())),
            read(invalidated(), Err(TestError::Failed)),
        ]),
        Err(SlotWriteError::Storage(
            TestError::Failed
        ))
    );
}

#[test]
fn invalidation_mismatch_prevents_body_write() {
    for magic in [[0x4A, 0x01], [0, 1], [0xFF, 0xFF]] {
        let mut bytes = invalidated();
        bytes[..2].copy_from_slice(&magic);

        assert_eq!(
            run(vec![
                write(0, &[0, 0], Ok(())),
                read(bytes, Ok(())),
            ]),
            Err(SlotWriteError::InvalidationFailed)
        );
    }
}

#[test]
fn body_write_error_prevents_restoring_valid_magic() {
    let candidate = candidate();

    assert_eq!(
        run(vec![
            write(0, &[0, 0], Ok(())),
            read(invalidated(), Ok(())),
            write(
                2,
                &candidate[2..],
                Err(TestError::Failed)
            ),
        ]),
        Err(SlotWriteError::Storage(
            TestError::Failed
        ))
    );
}

#[test]
fn final_magic_write_error_aborts_without_retry() {
    let candidate = candidate();

    assert_eq!(
        run(vec![
            write(0, &[0, 0], Ok(())),
            read(invalidated(), Ok(())),
            write(2, &candidate[2..], Ok(())),
            write(
                0,
                &candidate[..2],
                Err(TestError::Failed)
            ),
        ]),
        Err(SlotWriteError::Storage(
            TestError::Failed
        ))
    );
}

#[test]
fn final_read_error_prevents_success_even_with_matching_bytes() {
    let mut steps = success_steps();

    *steps.last_mut().unwrap() = read(candidate(), Err(TestError::Failed));

    assert_eq!(
        run(steps),
        Err(SlotWriteError::Storage(
            TestError::Failed
        ))
    );
}

#[test]
fn final_readback_must_match_magic_sequence_data_crc_and_padding() {
    // Two entries: inner CRC at 56, outer CRC at 60.
    for index in [0, 4, 20, 56, 60, SLOT_SIZE - 1] {
        let mut steps = success_steps();
        let mut changed = candidate();
        changed[index] ^= 1;

        *steps.last_mut().unwrap() = read(changed, Ok(()));

        assert_eq!(
            run(steps),
            Err(SlotWriteError::VerificationFailed),
            "Changed byte: {index}",
        );
    }
}

#[test]
fn another_valid_record_is_not_accepted_as_the_requested_record() {
    let mut other = [0; SLOT_SIZE];

    encode_slot(
        26,
        &[Some(calibration()); 2],
        &mut other,
        &mut SoftwareCrc32,
    )
    .unwrap();

    let mut steps = success_steps();
    *steps.last_mut().unwrap() = read(other, Ok(()));

    assert_eq!(
        run(steps),
        Err(SlotWriteError::VerificationFailed)
    );
}
