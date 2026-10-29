use super::{JournalLayout, LayoutError, StorageRange};
use crate::{JOURNAL_SIZE, SECTOR_COUNT, SECTOR_SIZE, SLOT_COUNT, SLOT_SIZE};

fn layout() -> JournalLayout {
    JournalLayout::new(0x2000, 0x20000).unwrap()
}

#[test]
fn aligned_regions_including_exact_storage_end_are_accepted() {
    for (start, storage_size) in [
        (0, JOURNAL_SIZE as u32),
        (0x2000, 0x4000),
        (0x2000, 0x20000),
    ] {
        assert!(
            JournalLayout::new(start, storage_size).is_ok(),
            "start: {start:#x}, storage size: {storage_size:#x}",
        );
    }
}

#[test]
fn misaligned_start_is_rejected() {
    for start in [1, 0x0FFF, 0x1001] {
        assert_eq!(
            JournalLayout::new(start, 0x20000),
            Err(LayoutError::MisalignedStart),
        );
    }
}

#[test]
fn region_outside_storage_is_rejected() {
    for (start, storage_size) in [
        (0, 0),
        (0, JOURNAL_SIZE as u32 - 1),
        (0x2000, 0x3FFF),
        (0x20000, 0x20000),
    ] {
        assert_eq!(
            JournalLayout::new(start, storage_size),
            Err(LayoutError::RegionOutOfBounds),
        );
    }
}

#[test]
fn overflowing_region_end_is_rejected() {
    assert_eq!(
        JournalLayout::new(0xFFFF_F000, u32::MAX),
        Err(LayoutError::RegionOutOfBounds),
    );
}

#[test]
fn first_slot_starts_at_region_start() {
    assert_eq!(
        layout().slot_range(0),
        Ok(StorageRange {
            offset: 0x2000,
            length: SLOT_SIZE,
        }),
    );
}

#[test]
fn last_slot_ends_at_region_end() {
    let range = layout()
        .slot_range(SLOT_COUNT - 1)
        .unwrap();

    assert_eq!(
        range,
        StorageRange {
            offset: 0x3E00,
            length: SLOT_SIZE,
        },
    );
    assert_eq!(
        range.offset + range.length as u32,
        0x4000
    );
}

#[test]
fn invalid_slot_indices_are_rejected_before_arithmetic() {
    for index in [SLOT_COUNT, usize::MAX] {
        assert_eq!(
            layout().slot_range(index),
            Err(LayoutError::UnknownSlot(index)),
        );
    }
}

#[test]
fn first_sector_starts_at_region_start() {
    assert_eq!(
        layout().sector_range(0),
        Ok(StorageRange {
            offset: 0x2000,
            length: SECTOR_SIZE,
        }),
    );
}

#[test]
fn last_sector_ends_at_region_end() {
    let range = layout()
        .sector_range(SECTOR_COUNT - 1)
        .unwrap();

    assert_eq!(
        range,
        StorageRange {
            offset: 0x3000,
            length: SECTOR_SIZE,
        },
    );
    assert_eq!(
        range.offset + range.length as u32,
        0x4000
    );
}

#[test]
fn invalid_sector_indices_are_rejected_before_arithmetic() {
    for index in [SECTOR_COUNT, usize::MAX] {
        assert_eq!(
            layout().sector_range(index),
            Err(LayoutError::UnknownSector(index)),
        );
    }
}
