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

pub use storage::{
    JOURNAL_SIZE, JournalRead, JournalStorage, SECTOR_COUNT, SECTOR_SIZE,
    SLOT_COUNT, SLOTS_PER_SECTOR,
};

mod journal;

pub use journal::{JournalError, append, load, save_if_changed};

mod journal_layout;

pub use journal_layout::{JournalLayout, LayoutError, StorageRange};

mod calibration_storage;

pub use calibration_storage::{CalibrationRead, CalibrationStorage};

mod slot_layout;

pub use slot_layout::{SlotLayout, SlotLayoutError};

mod calibration_load;

pub use calibration_load::{CalibrationLoadError, load_calibrations};

mod calibration_scan;

pub use calibration_scan::scan_calibrations;

mod calibration_write_plan;

pub use calibration_write_plan::{
    CalibrationWritePlan, CalibrationWritePlanError, plan_calibration_write,
};

mod slot_write;

pub use journal_slot::write_calibration_slot;
pub use slot_write::{SlotWrite, SlotWriteError};

mod calibration_save;

pub use calibration_save::{CalibrationSaveError, save_calibrations};
