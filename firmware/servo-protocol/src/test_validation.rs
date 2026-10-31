use super::*;
use crate::Opcode;

fn regions() -> [RegisterRegion; 4] {
    [
        RegisterRegion {
            first_index: 0x01,
            entries: 1,
            bytes_per_entry: 1,
            access: Access::ReadOnly,
        },
        RegisterRegion {
            first_index: 0x03,
            entries: 1,
            bytes_per_entry: 1,
            access: Access::ReadWriteBits,
        },
        RegisterRegion {
            first_index: 0x10,
            entries: 1,
            bytes_per_entry: 368,
            access: Access::ReadWrite,
        },
        RegisterRegion {
            first_index: 0x20,
            entries: 18,
            bytes_per_entry: 4,
            access: Access::ReadWrite,
        },
    ]
}

fn header(opcode: Opcode, index: u8, length: u16) -> Header {
    Header {
        opcode,
        index,
        length,
    }
}

#[test]
fn status_accepts_exact_read_width() {
    assert_eq!(
        validate_header(
            &header(Opcode::Read, 0x01, 1),
            &regions()
        ),
        Ok(()),
    );
}

#[test]
fn readonly_field_rejects_all_modifying_operations() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        assert_eq!(
            validate_header(&header(opcode, 0x01, 1), &regions()),
            Err(ValidationError::UnsupportedAccess),
        );
    }
}

#[test]
fn control_accepts_all_four_operations() {
    for opcode in [Opcode::Read, Opcode::Write, Opcode::Set, Opcode::Clear] {
        assert_eq!(
            validate_header(&header(opcode, 0x03, 1), &regions()),
            Ok(()),
        );
    }
}

#[test]
fn calibration_accepts_only_complete_read_or_write() {
    for opcode in [Opcode::Read, Opcode::Write] {
        assert_eq!(
            validate_header(&header(opcode, 0x10, 368), &regions()),
            Ok(()),
        );

        for length in [0, 1, 367, 369] {
            assert_eq!(
                validate_header(
                    &header(opcode, 0x10, length),
                    &regions()
                ),
                Err(ValidationError::InvalidLength),
            );
        }
    }
}

#[test]
fn ordinary_regions_reject_bit_operations() {
    for index in [0x10, 0x20] {
        for opcode in [Opcode::Set, Opcode::Clear] {
            assert_eq!(
                validate_header(&header(opcode, index, 1), &regions()),
                Err(ValidationError::UnsupportedAccess),
            );
        }
    }
}

#[test]
fn unknown_indices_are_rejected() {
    for index in [0, 0x02, 0x11, 0x1F, 0x32, 0xFF] {
        assert_eq!(
            validate_header(
                &header(Opcode::Read, index, 4),
                &regions()
            ),
            Err(ValidationError::UnknownIndex),
        );
    }
}

#[test]
fn angle_array_accepts_single_partial_and_complete_ranges() {
    for opcode in [Opcode::Read, Opcode::Write] {
        for (index, length) in [(0x20, 4), (0x20, 72), (0x25, 12), (0x31, 4)] {
            assert_eq!(
                validate_header(
                    &header(opcode, index, length),
                    &regions()
                ),
                Ok(()),
            );
        }
    }
}

#[test]
fn angle_lengths_require_complete_nonempty_values() {
    for length in [0, 1, 2, 3, 5, u16::MAX] {
        assert_eq!(
            validate_header(
                &header(Opcode::Write, 0x20, length),
                &regions(),
            ),
            Err(ValidationError::InvalidLength),
        );
    }
}

#[test]
fn accesses_cannot_cross_a_region_boundary() {
    for (index, length) in [(0x01, 2), (0x10, 736), (0x20, 76), (0x31, 8)] {
        assert_eq!(
            validate_header(
                &header(Opcode::Read, index, length),
                &regions(),
            ),
            Err(ValidationError::OutOfBounds),
        );
    }
}

#[test]
fn last_index_is_usable_without_wrapping() {
    let map = [RegisterRegion {
        first_index: 0xFE,
        entries: 2,
        bytes_per_entry: 4,
        access: Access::ReadWrite,
    }];

    assert_eq!(
        validate_header(&header(Opcode::Write, 0xFE, 8), &map),
        Ok(()),
    );
    assert_eq!(
        validate_header(&header(Opcode::Write, 0xFF, 4), &map),
        Ok(()),
    );
    assert_eq!(
        validate_header(&header(Opcode::Write, 0xFF, 8), &map),
        Err(ValidationError::OutOfBounds),
    );
}

#[test]
fn invalid_region_definitions_are_rejected() {
    for region in [
        RegisterRegion {
            first_index: 0,
            entries: 0,
            bytes_per_entry: 4,
            access: Access::ReadOnly,
        },
        RegisterRegion {
            first_index: 0,
            entries: 1,
            bytes_per_entry: 0,
            access: Access::ReadOnly,
        },
        RegisterRegion {
            first_index: 0xFF,
            entries: 2,
            bytes_per_entry: 4,
            access: Access::ReadOnly,
        },
    ] {
        assert_eq!(
            validate_header(&header(Opcode::Read, 0, 4), &[region]),
            Err(ValidationError::InvalidMap),
        );
    }
}

#[test]
fn overlapping_regions_are_rejected() {
    let map = [
        RegisterRegion {
            first_index: 0x20,
            entries: 18,
            bytes_per_entry: 4,
            access: Access::ReadWrite,
        },
        RegisterRegion {
            first_index: 0x25,
            entries: 1,
            bytes_per_entry: 1,
            access: Access::ReadOnly,
        },
    ];

    assert_eq!(
        validate_header(&header(Opcode::Read, 0x20, 4), &map),
        Err(ValidationError::InvalidMap),
    );
}
