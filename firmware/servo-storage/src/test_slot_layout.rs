use super::{SlotLayout, SlotLayoutError};
use crate::StorageRange;

#[test]
fn full_eeprom_contains_64_slots() {
    let layout = SlotLayout::new(0, 64, 32_768).unwrap();

    assert_eq!(layout.slot_count(), 64);
}

#[test]
fn first_slot_starts_at_zero() {
    let layout = SlotLayout::new(0, 64, 32_768).unwrap();

    assert_eq!(
        layout.slot_range(0),
        Ok(StorageRange {
            offset: 0,
            length: 512,
        })
    );
}

#[test]
fn last_slot_ends_exactly_at_eeprom_capacity() {
    let layout = SlotLayout::new(0, 64, 32_768).unwrap();

    assert_eq!(
        layout.slot_range(63),
        Ok(StorageRange {
            offset: 32_256,
            length: 512,
        })
    );
}

#[test]
fn all_slots_are_consecutive_and_page_aligned() {
    let layout = SlotLayout::new(0, 64, 32_768).unwrap();

    for index in 0..64 {
        let range = layout.slot_range(index).unwrap();

        assert_eq!(range.offset, index as u32 * 512);
        assert_eq!(range.length, 512);
        assert_eq!(range.offset % 64, 0);
    }
}

#[test]
fn one_slot_region_can_start_at_a_nonzero_offset() {
    let layout = SlotLayout::new(512, 1, 1_024).unwrap();

    assert_eq!(layout.slot_count(), 1);
    assert_eq!(
        layout.slot_range(0),
        Ok(StorageRange {
            offset: 512,
            length: 512,
        })
    );
}

#[test]
fn region_may_leave_unused_storage_before_and_after_it() {
    let layout = SlotLayout::new(1_024, 2, 32_768).unwrap();

    assert_eq!(layout.slot_count(), 2);
    assert_eq!(
        layout.slot_range(1),
        Ok(StorageRange {
            offset: 1_536,
            length: 512,
        })
    );
}

#[test]
fn empty_region_is_rejected() {
    assert_eq!(
        SlotLayout::new(0, 0, 32_768),
        Err(SlotLayoutError::EmptyRegion)
    );
}

#[test]
fn start_must_be_aligned_to_a_slot_boundary() {
    for start in [1, 64, 511, 513] {
        assert_eq!(
            SlotLayout::new(start, 1, 32_768),
            Err(SlotLayoutError::MisalignedStart)
        );
    }
}

#[test]
fn region_larger_than_available_storage_is_rejected() {
    for (start, count, capacity) in [
        (0, 65, 32_768),
        (512, 64, 32_768),
        (0, 1, 511),
        (1_024, 1, 512),
        (0, 1, 0),
    ] {
        assert_eq!(
            SlotLayout::new(start, count, capacity),
            Err(SlotLayoutError::RegionOutOfBounds)
        );
    }
}

#[test]
fn address_addition_overflow_is_rejected() {
    assert_eq!(
        SlotLayout::new(0xFFFF_FE00, 1, u32::MAX),
        Err(SlotLayoutError::RegionOutOfBounds)
    );
}

#[test]
fn excessive_slot_count_is_rejected() {
    assert_eq!(
        SlotLayout::new(0, usize::MAX, u32::MAX),
        Err(SlotLayoutError::RegionOutOfBounds)
    );
}

#[test]
fn slot_at_region_end_is_rejected() {
    let layout = SlotLayout::new(0, 64, 32_768).unwrap();

    assert_eq!(
        layout.slot_range(64),
        Err(SlotLayoutError::UnknownSlot(64))
    );
}

#[test]
fn excessive_slot_index_is_rejected_before_arithmetic() {
    let layout = SlotLayout::new(0, 64, 32_768).unwrap();

    assert_eq!(
        layout.slot_range(usize::MAX),
        Err(SlotLayoutError::UnknownSlot(usize::MAX))
    );
}
