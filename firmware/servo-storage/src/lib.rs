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
