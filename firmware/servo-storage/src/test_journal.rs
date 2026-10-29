extern crate std;

use std::vec::Vec;

use hexaroam_servo_control::{Angle, Calibration};

use super::{JournalError, append};
use crate::{
    JournalRead, JournalRecord, JournalStorage, LocatedRecord, SLOT_COUNT,
    SLOT_SIZE, SLOTS_PER_SECTOR, ScanError, SlotError, SoftwareCrc32,
    encode_slot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageError {
    Read,
    Write,
    Erase,
}

struct MemoryStorage {
    slots: [[u8; SLOT_SIZE]; SLOT_COUNT],
    operations: Vec<(u8, usize)>,
    fail_read_slot: Option<usize>,
    fail_write: bool,
    fail_erase: bool,
    fail_readback: bool,
    corrupt_write_at: Option<usize>,
    wrote: bool,
}

impl MemoryStorage {
    fn new() -> Self {
        Self {
            slots: [[0xFF; SLOT_SIZE]; SLOT_COUNT],
            operations: Vec::new(),
            fail_read_slot: None,
            fail_write: false,
            fail_erase: false,
            fail_readback: false,
            corrupt_write_at: None,
            wrote: false,
        }
    }

    fn seed<const N: usize>(
        &mut self,
        index: usize,
        sequence: u64,
        calibrations: [Option<Calibration>; N],
    ) {
        self.slots[index] = encoded(sequence, calibrations);
    }

    fn fill_sector(&mut self, sector: usize, first_sequence: u64) {
        let start = sector * SLOTS_PER_SECTOR;

        for local_index in 0..SLOTS_PER_SECTOR {
            self.seed(
                start + local_index,
                first_sequence + local_index as u64,
                [Some(calibration())],
            );
        }
    }
}

impl JournalRead for MemoryStorage {
    type Error = StorageError;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        if self.fail_read_slot == Some(slot_index)
            || (self.fail_readback && self.wrote)
        {
            return Err(StorageError::Read);
        }

        output.copy_from_slice(&self.slots[slot_index]);
        Ok(())
    }
}

impl JournalStorage for MemoryStorage {
    fn write_slot(
        &mut self,
        slot_index: usize,
        data: &[u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        // Catch attempts to overwrite even partially used slots.
        assert!(
            self.slots[slot_index]
                .iter()
                .all(|&byte| byte == 0xFF)
        );

        self.operations.push((b'W', slot_index));
        self.wrote = true;

        if self.fail_write {
            self.slots[slot_index][..10].copy_from_slice(&data[..10]);
            return Err(StorageError::Write);
        }

        self.slots[slot_index].copy_from_slice(data);

        if let Some(offset) = self.corrupt_write_at {
            self.slots[slot_index][offset] ^= 1;
        }

        Ok(())
    }

    fn erase_sector(&mut self, sector_index: usize) -> Result<(), Self::Error> {
        let start = sector_index * SLOTS_PER_SECTOR;
        self.operations
            .push((b'E', sector_index));

        if self.fail_erase {
            // Simulate a partially completed erase.
            self.slots[start].fill(0xFF);
            return Err(StorageError::Erase);
        }

        for slot in &mut self.slots[start..start + SLOTS_PER_SECTOR] {
            slot.fill(0xFF);
        }

        Ok(())
    }
}

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(9_000),
        2_000,
    )
    .unwrap()
}

fn encoded<const N: usize>(
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

fn expected<const N: usize>(
    slot_index: usize,
    sequence: u64,
    calibrations: [Option<Calibration>; N],
) -> Result<LocatedRecord<N>, JournalError<StorageError>> {
    Ok(LocatedRecord {
        slot_index,
        record: JournalRecord {
            sequence,
            calibrations,
        },
    })
}

#[test]
fn empty_storage_starts_at_first_slot_with_sequence_one() {
    let mut storage = MemoryStorage::new();

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        expected(0, 1, [Some(calibration())]),
    );

    assert_eq!(
        storage.operations.as_slice(),
        &[(b'W', 0)]
    );
    assert_eq!(
        storage.slots[0],
        encoded(1, [Some(calibration())]),
    );
}

#[test]
fn append_uses_next_slot_and_preserves_previous_record() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, 10, [Some(calibration())]);
    let previous = storage.slots[0];

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        expected(1, 11, [Some(calibration())]),
    );

    assert_eq!(storage.slots[0], previous);
    assert_eq!(
        storage.operations.as_slice(),
        &[(b'W', 1)]
    );
}

#[test]
fn damaged_slots_and_padding_only_changes_are_not_reused() {
    for changed_byte in [0, SLOT_SIZE - 1] {
        let mut storage = MemoryStorage::new();
        storage.seed(0, 10, [Some(calibration())]);

        // Slot1 is an erased hole; slot2 is partially used.
        storage.slots[2][changed_byte] = 0;

        assert_eq!(
            append(
                &mut storage,
                &[Some(calibration())],
                &mut SoftwareCrc32,
            ),
            expected(3, 11, [Some(calibration())]),
        );

        assert_eq!(
            storage.operations.as_slice(),
            &[(b'W', 3)]
        );
    }
}

#[test]
fn highest_sequence_determines_active_sector() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, 4, [Some(calibration())]);
    storage.seed(8, 20, [Some(calibration())]);

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        expected(9, 21, [Some(calibration())]),
    );

    assert_eq!(
        storage.operations.as_slice(),
        &[(b'W', 9)]
    );
}

#[test]
fn partial_configuration_is_written_as_newest_record() {
    let mut storage = MemoryStorage::new();
    storage.seed(
        0,
        10,
        [Some(calibration()), Some(calibration())],
    );

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration()), None],
            &mut SoftwareCrc32,
        ),
        expected(1, 11, [Some(calibration()), None]),
    );

    assert_eq!(
        storage.slots[1],
        encoded(11, [Some(calibration()), None]),
    );
}

#[test]
fn full_sector_switches_without_erasing_an_erased_sector() {
    let mut storage = MemoryStorage::new();
    storage.fill_sector(0, 1);
    let previous = storage.slots[7];

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        expected(8, 9, [Some(calibration())]),
    );

    assert_eq!(storage.slots[7], previous);
    assert_eq!(
        storage.operations.as_slice(),
        &[(b'W', 8)]
    );
}

#[test]
fn sector_switch_erases_only_the_other_sector() {
    for active_sector in [0, 1] {
        let mut storage = MemoryStorage::new();
        let other_sector = 1 - active_sector;
        let target = other_sector * SLOTS_PER_SECTOR;
        let previous_index =
            active_sector * SLOTS_PER_SECTOR + SLOTS_PER_SECTOR - 1;

        storage.seed(target, 1, [Some(calibration())]);
        storage.fill_sector(active_sector, 10);
        let previous = storage.slots[previous_index];

        assert_eq!(
            append(
                &mut storage,
                &[Some(calibration())],
                &mut SoftwareCrc32,
            ),
            expected(target, 18, [Some(calibration())]),
        );

        assert_eq!(storage.slots[previous_index], previous);
        assert_eq!(
            storage.operations.as_slice(),
            &[(b'E', other_sector), (b'W', target)],
        );
    }
}

#[test]
fn duplicate_sequences_prevent_storage_changes() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, 10, [Some(calibration())]);
    storage.seed(8, 10, [Some(calibration())]);

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        Err(JournalError::Scan(
            ScanError::DuplicateSequence(10)
        )),
    );

    assert!(storage.operations.is_empty());
}

#[test]
fn sequence_overflow_prevents_storage_changes() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, u64::MAX, [Some(calibration())]);

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        Err(JournalError::SequenceOverflow),
    );

    assert!(storage.operations.is_empty());
}

#[test]
fn scan_read_error_prevents_storage_changes() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, 10, [Some(calibration())]);
    storage.fail_read_slot = Some(5);

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        Err(JournalError::Storage(
            StorageError::Read
        )),
    );

    assert!(storage.operations.is_empty());
}

#[test]
fn failed_write_preserves_previous_record_and_retry_skips_slot() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, 10, [Some(calibration())]);
    let previous = storage.slots[0];
    storage.fail_write = true;

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        Err(JournalError::Storage(
            StorageError::Write
        )),
    );

    assert_eq!(storage.slots[0], previous);

    storage.fail_write = false;

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        expected(2, 11, [Some(calibration())]),
    );

    assert_eq!(
        storage.operations.as_slice(),
        &[(b'W', 1), (b'W', 2)],
    );
}

#[test]
fn failed_erase_preserves_latest_record_and_prevents_write() {
    let mut storage = MemoryStorage::new();
    storage.fill_sector(0, 10);
    storage.seed(8, 1, [Some(calibration())]);
    storage.slots[SLOT_COUNT - 1][0] = 0;
    let previous = storage.slots[7];
    storage.fail_erase = true;

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        Err(JournalError::Storage(
            StorageError::Erase
        )),
    );

    assert_eq!(storage.slots[7], previous);
    assert_eq!(
        storage.operations.as_slice(),
        &[(b'E', 1)]
    );
}

#[test]
fn readback_error_is_reported_as_storage_error() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, 10, [Some(calibration())]);
    let previous = storage.slots[0];
    storage.fail_readback = true;

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        Err(JournalError::Storage(
            StorageError::Read
        )),
    );

    assert_eq!(storage.slots[0], previous);
}

#[test]
fn mismatched_readback_is_rejected() {
    for offset in [0, SLOT_SIZE - 1] {
        let mut storage = MemoryStorage::new();
        storage.seed(0, 10, [Some(calibration())]);
        let previous = storage.slots[0];
        storage.corrupt_write_at = Some(offset);

        assert_eq!(
            append(
                &mut storage,
                &[Some(calibration())],
                &mut SoftwareCrc32,
            ),
            Err(JournalError::VerificationFailed),
            "corrupted byte: {offset}",
        );

        assert_eq!(storage.slots[0], previous);
    }
}

#[test]
fn oversized_record_is_rejected_before_erasing_or_writing() {
    let mut storage = MemoryStorage::new();
    storage.fill_sector(0, 10);
    storage.seed(8, 1, [Some(calibration())]);
    let before = storage.slots;

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration()); 25],
            &mut SoftwareCrc32,
        ),
        Err(JournalError::Slot(
            SlotError::PayloadTooLarge
        )),
    );

    assert!(storage.operations.is_empty());
    assert_eq!(storage.slots, before);
}

#[test]
fn last_available_sequence_can_be_written() {
    let mut storage = MemoryStorage::new();
    storage.seed(0, u64::MAX - 1, [Some(calibration())]);

    assert_eq!(
        append(
            &mut storage,
            &[Some(calibration())],
            &mut SoftwareCrc32,
        ),
        expected(1, u64::MAX, [Some(calibration())]),
    );
}
