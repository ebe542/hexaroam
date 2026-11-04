use super::*;
use crate::{CalibrationPersistenceError, persist_calibration};
use hexaroam_servo_storage::{
    JournalError, JournalRead, JournalStorage, SLOT_COUNT, SLOT_SIZE,
    SLOTS_PER_SECTOR, encode_slot, load,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageError {
    Read,
    Write,
}

struct MemoryStorage {
    slots: [[u8; SLOT_SIZE]; SLOT_COUNT],
    reads: usize,
    writes: usize,
    erases: usize,
    fail_read: bool,
    fail_write: bool,
    corrupt_write: bool,
}

impl MemoryStorage {
    fn new() -> Self {
        Self {
            slots: [[0xFF; SLOT_SIZE]; SLOT_COUNT],
            reads: 0,
            writes: 0,
            erases: 0,
            fail_read: false,
            fail_write: false,
            corrupt_write: false,
        }
    }

    fn assert_untouched(&self) {
        assert_eq!(self.reads, 0);
        assert_eq!(self.writes, 0);
        assert_eq!(self.erases, 0);
        assert!(
            self.slots
                .iter()
                .flatten()
                .all(|&byte| byte == 0xFF)
        );
    }
}

impl JournalRead for MemoryStorage {
    type Error = StorageError;

    fn read_slot(
        &mut self,
        index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        self.reads += 1;

        if self.fail_read {
            return Err(StorageError::Read);
        }

        output.copy_from_slice(&self.slots[index]);
        Ok(())
    }
}

impl JournalStorage for MemoryStorage {
    fn write_slot(
        &mut self,
        index: usize,
        data: &[u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        self.writes += 1;
        assert!(
            self.slots[index]
                .iter()
                .all(|&byte| byte == 0xFF)
        );

        if self.fail_write {
            return Err(StorageError::Write);
        }

        self.slots[index].copy_from_slice(data);

        if self.corrupt_write {
            self.slots[index][0] ^= 1;
        }

        Ok(())
    }

    fn erase_sector(&mut self, index: usize) -> Result<(), Self::Error> {
        self.erases += 1;
        let start = index * SLOTS_PER_SECTOR;

        for slot in &mut self.slots[start..start + SLOTS_PER_SECTOR] {
            slot.fill(0xFF);
        }

        Ok(())
    }
}

fn release_request(processor: &mut Processor, transport: &mut TestTransport) {
    queue_calibration(processor, transport);
    poll(processor, transport, 0, 100);

    assert_response(&transport.outgoing, 0x10, &[]);
    assert_ready(processor);
}

fn assert_result(
    processor: &mut Processor,
    transport: &mut TestTransport,
    result: CalibrationSaveResult,
) {
    assert!(!processor.response_pending());
    transport.outgoing.clear();

    queue_read(processor, transport, 1, 4);
    poll(processor, transport, 0, 100);

    let expected = STATUS_CALIBRATION_VALID | ((result as u32) << 8);

    assert_response(
        &transport.outgoing,
        1,
        &expected.to_le_bytes(),
    );
    assert!(processor.calibration_ready().is_none());
}

#[test]
fn idle_does_not_access_storage() {
    let (mut processor, _) = processor();
    let mut storage = MemoryStorage::new();

    assert_eq!(
        persist_calibration(
            &mut processor,
            &mut storage,
            &mut SoftwareCrc32,
        ),
        Ok(false),
    );

    storage.assert_untouched();
}

#[test]
fn queued_or_partial_ack_prevents_every_storage_access() {
    for budget in [0, 7] {
        let (mut processor, _) = processor();
        let mut transport = TestTransport::default();
        let mut storage = MemoryStorage::new();

        queue_calibration(&mut processor, &mut transport);
        poll(
            &mut processor,
            &mut transport,
            0,
            budget,
        );

        assert_eq!(
            persist_calibration(
                &mut processor,
                &mut storage,
                &mut SoftwareCrc32,
            ),
            Ok(false),
        );

        storage.assert_untouched();
        assert!(processor.response_pending());
    }
}

#[test]
fn successful_save_completes_request_only_once() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();
    let mut storage = MemoryStorage::new();

    release_request(&mut processor, &mut transport);

    assert_eq!(
        persist_calibration(
            &mut processor,
            &mut storage,
            &mut SoftwareCrc32,
        ),
        Ok(true),
    );

    assert_eq!(storage.writes, 1);
    assert_eq!(storage.erases, 0);

    let latest = load::<18, _>(&mut storage, &mut SoftwareCrc32)
        .unwrap()
        .unwrap();

    assert_eq!(latest.record.sequence, 1);
    assert_eq!(
        latest.record.calibrations,
        [Some(calibration()); 18],
    );

    let reads = storage.reads;

    assert_eq!(
        persist_calibration(
            &mut processor,
            &mut storage,
            &mut SoftwareCrc32,
        ),
        Ok(false),
    );

    assert_eq!(storage.reads, reads);
    assert_eq!(storage.writes, 1);
    assert_eq!(trace.starts.get(), 0);
    assert_eq!(trace.writes.get(), 0);

    assert_result(
        &mut processor,
        &mut transport,
        CalibrationSaveResult::Success,
    );
}

#[test]
fn repeated_identical_request_does_not_write_again() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    let mut storage = MemoryStorage::new();

    for _ in 0..2 {
        transport.outgoing.clear();
        release_request(&mut processor, &mut transport);

        assert_eq!(
            persist_calibration(
                &mut processor,
                &mut storage,
                &mut SoftwareCrc32,
            ),
            Ok(true),
        );
    }

    assert_eq!(storage.writes, 1);
    assert_eq!(storage.erases, 0);

    let latest = load::<18, _>(&mut storage, &mut SoftwareCrc32)
        .unwrap()
        .unwrap();

    assert_eq!(latest.record.sequence, 1);

    assert_result(
        &mut processor,
        &mut transport,
        CalibrationSaveResult::Success,
    );
}

#[test]
fn changed_persisted_data_creates_next_record() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    let mut storage = MemoryStorage::new();

    let previous = Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_100,
        Angle::from_centidegrees(4_500),
        2_100,
    )
    .unwrap();

    encode_slot(
        7,
        &[Some(previous); 18],
        &mut storage.slots[0],
        &mut SoftwareCrc32,
    )
    .unwrap();

    let old_slot = storage.slots[0];
    release_request(&mut processor, &mut transport);

    assert_eq!(
        persist_calibration(
            &mut processor,
            &mut storage,
            &mut SoftwareCrc32,
        ),
        Ok(true),
    );

    let latest = load::<18, _>(&mut storage, &mut SoftwareCrc32)
        .unwrap()
        .unwrap();

    assert_eq!(latest.record.sequence, 8);
    assert_eq!(
        latest.record.calibrations,
        [Some(calibration()); 18],
    );
    assert_eq!(storage.slots[0], old_slot);
    assert_eq!(storage.writes, 1);
    assert_eq!(storage.erases, 0);
}

#[test]
fn storage_errors_are_returned_and_reported_in_status() {
    for error in [StorageError::Read, StorageError::Write] {
        let (mut processor, _) = processor();
        let mut transport = TestTransport::default();
        let mut storage = MemoryStorage::new();

        storage.fail_read = error == StorageError::Read;
        storage.fail_write = error == StorageError::Write;

        release_request(&mut processor, &mut transport);

        assert_eq!(
            persist_calibration(
                &mut processor,
                &mut storage,
                &mut SoftwareCrc32,
            ),
            Err(CalibrationPersistenceError::Journal(
                JournalError::Storage(error),
            )),
        );

        assert_result(
            &mut processor,
            &mut transport,
            CalibrationSaveResult::StorageError,
        );
    }
}

#[test]
fn verification_error_has_distinct_status_result() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    let mut storage = MemoryStorage::new();
    storage.corrupt_write = true;

    release_request(&mut processor, &mut transport);

    assert_eq!(
        persist_calibration(
            &mut processor,
            &mut storage,
            &mut SoftwareCrc32,
        ),
        Err(CalibrationPersistenceError::Journal(
            JournalError::VerificationFailed,
        )),
    );

    assert_result(
        &mut processor,
        &mut transport,
        CalibrationSaveResult::VerificationFailed,
    );
}

#[test]
fn failed_request_is_not_automatically_retried() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    let mut storage = MemoryStorage::new();
    storage.fail_write = true;

    release_request(&mut processor, &mut transport);

    assert!(
        persist_calibration(
            &mut processor,
            &mut storage,
            &mut SoftwareCrc32,
        )
        .is_err()
    );

    let reads = storage.reads;
    let writes = storage.writes;

    assert_eq!(
        persist_calibration(
            &mut processor,
            &mut storage,
            &mut SoftwareCrc32,
        ),
        Ok(false),
    );

    assert_eq!(storage.reads, reads);
    assert_eq!(storage.writes, writes);
}
