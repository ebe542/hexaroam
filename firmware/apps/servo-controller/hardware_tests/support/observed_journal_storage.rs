//! Test-only counters around the real flash journal backend.

use crate::flash_journal_storage::{FlashJournalStorage, FlashStorageError};
use hexaroam_servo_storage::{JournalRead, JournalStorage, SLOT_SIZE};

pub struct ObservedJournalStorage {
    inner: FlashJournalStorage,
    write_calls: usize,
    erase_calls: usize,
}

impl ObservedJournalStorage {
    pub fn new(inner: FlashJournalStorage) -> Self {
        Self {
            inner,
            write_calls: 0,
            erase_calls: 0,
        }
    }

    pub fn write_calls(&self) -> usize {
        self.write_calls
    }

    pub fn erase_calls(&self) -> usize {
        self.erase_calls
    }
}

impl JournalRead for ObservedJournalStorage {
    type Error = FlashStorageError;

    fn read_slot(
        &mut self,
        index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        self.inner.read_slot(index, output)
    }
}

impl JournalStorage for ObservedJournalStorage {
    fn write_slot(
        &mut self,
        index: usize,
        data: &[u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        // Count attempted operations, including failures.
        self.write_calls += 1;
        self.inner.write_slot(index, data)
    }

    fn erase_sector(&mut self, index: usize) -> Result<(), Self::Error> {
        self.erase_calls += 1;
        self.inner.erase_sector(index)
    }
}
