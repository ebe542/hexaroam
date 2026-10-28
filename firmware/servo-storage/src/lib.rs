#![no_std]

//! Hardware-independent calibration data encoding and storage.

mod crc32;

pub use crc32::{Checksum, SoftwareCrc32};

mod format;

pub use format::{FormatError, decode, encode};

mod journal_slot;

pub use journal_slot::{
    JournalRecord, SLOT_SIZE, SlotError, decode_slot, encode_slot,
};

mod journal_scan;

pub use journal_scan::{LocatedRecord, ScanError, find_latest};

mod storage;

pub use storage::{JournalStorage, SECTOR_COUNT, SLOT_COUNT, SLOTS_PER_SECTOR};

mod journal;

pub use journal::{JournalError, append};
