use hexaroam_servo_control::{Angle, Calibration};

use super::{JournalError, load};
use crate::{
    JournalRead, JournalRecord, LocatedRecord, SLOT_COUNT, SLOT_SIZE,
    ScanError, SoftwareCrc32, encode_slot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadError {
    Failed,
}

// Deliberately implements only JournalRead.
struct MemoryReader {
    slots: [[u8; SLOT_SIZE]; SLOT_COUNT],
    fail_read_at: Option<usize>,
}

impl MemoryReader {
    fn new() -> Self {
        Self {
            slots: [[0xFF; SLOT_SIZE]; SLOT_COUNT],
            fail_read_at: None,
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

impl JournalRead for MemoryReader {
    type Error = ReadError;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        if self.fail_read_at == Some(slot_index) {
            return Err(ReadError::Failed);
        }

        output.copy_from_slice(&self.slots[slot_index]);
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

#[test]
fn no_valid_record_returns_none() {
    for fill in [0xFF, 0] {
        let mut reader = MemoryReader::new();
        reader.slots = [[fill; SLOT_SIZE]; SLOT_COUNT];

        assert_eq!(
            load::<1, _>(&mut reader, &mut SoftwareCrc32),
            Ok(None),
        );
    }
}

#[test]
fn newest_record_is_loaded_from_read_only_storage() {
    let mut reader = MemoryReader::new();
    reader.seed(0, 20, [Some(calibration())]);
    reader.seed(8, 10, [Some(calibration())]);

    assert_eq!(
        load::<1, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(LocatedRecord {
            slot_index: 0,
            record: JournalRecord {
                sequence: 20,
                calibrations: [Some(calibration())],
            },
        })),
    );
}

#[test]
fn newer_invalidation_is_preserved() {
    let mut reader = MemoryReader::new();
    reader.seed(0, 10, [Some(calibration())]);
    reader.seed(8, 11, [None::<Calibration>]);

    assert_eq!(
        load::<1, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(LocatedRecord {
            slot_index: 8,
            record: JournalRecord {
                sequence: 11,
                calibrations: [None],
            },
        })),
    );
}

#[test]
fn duplicate_sequence_is_reported_as_scan_error() {
    let mut reader = MemoryReader::new();
    reader.seed(0, 10, [Some(calibration())]);
    reader.seed(8, 10, [Some(calibration())]);

    assert_eq!(
        load::<1, _>(&mut reader, &mut SoftwareCrc32),
        Err(JournalError::Scan(
            ScanError::DuplicateSequence(10)
        )),
    );
}

#[test]
fn corrupt_record_is_skipped() {
    let mut reader = MemoryReader::new();
    reader.seed(0, 10, [Some(calibration())]);
    reader.seed(8, 11, [Some(calibration())]);
    reader.slots[8][4] ^= 1;

    assert_eq!(
        load::<1, _>(&mut reader, &mut SoftwareCrc32),
        Ok(Some(LocatedRecord {
            slot_index: 0,
            record: JournalRecord {
                sequence: 10,
                calibrations: [Some(calibration())],
            },
        })),
    );
}

#[test]
fn read_error_prevents_returning_an_earlier_record() {
    let mut reader = MemoryReader::new();
    reader.seed(0, 10, [Some(calibration())]);

    // A later unreadable slot might contain a newer record.
    reader.fail_read_at = Some(SLOT_COUNT - 1);

    assert_eq!(
        load::<1, _>(&mut reader, &mut SoftwareCrc32),
        Err(JournalError::Storage(ReadError::Failed)),
    );
}
