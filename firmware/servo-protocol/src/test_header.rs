use super::*;

#[test]
fn known_opcode_values_are_decoded() {
    for (value, expected) in [
        (1, Opcode::Read),
        (2, Opcode::Write),
        (3, Opcode::Set),
        (4, Opcode::Clear),
    ] {
        assert_eq!(Opcode::from_byte(value), Ok(expected));
    }
}

#[test]
fn every_other_opcode_value_is_rejected() {
    for value in 0_u8..=u8::MAX {
        if !(1..=4).contains(&value) {
            assert_eq!(
                Opcode::from_byte(value),
                Err(HeaderError::UnknownOpcode(value)),
            );
        }
    }
}

#[test]
fn header_decodes_calibration_length_in_little_endian() {
    // 368 bytes: the complete eighteen-servo calibration payload.
    assert_eq!(
        Header::decode(&[2, 0x10, 0x70, 0x01]),
        Ok(Header {
            opcode: Opcode::Write,
            index: 0x10,
            length: 368,
        }),
    );
}

#[test]
fn header_encodes_exact_wire_bytes() {
    let header = Header {
        opcode: Opcode::Write,
        index: 0x10,
        length: 368,
    };

    assert_eq!(header.encode(), [2, 0x10, 0x70, 0x01]);
}

#[test]
fn header_rejects_unknown_opcode() {
    assert_eq!(
        Header::decode(&[0xFF, 0x01, 1, 0]),
        Err(HeaderError::UnknownOpcode(0xFF)),
    );
}

#[test]
fn header_preserves_maximum_field_values() {
    let header = Header {
        opcode: Opcode::Read,
        index: u8::MAX,
        length: u16::MAX,
    };

    assert_eq!(header.encode(), [1, 0xFF, 0xFF, 0xFF]);
    assert_eq!(
        Header::decode(&[1, 0xFF, 0xFF, 0xFF]),
        Ok(header),
    );
}

#[test]
fn header_preserves_zero_length_for_separate_validation() {
    // Header decoding alone does not authorize a register access.
    let header = Header {
        opcode: Opcode::Read,
        index: 0,
        length: 0,
    };

    assert_eq!(
        Header::decode(&[1, 0, 0, 0]),
        Ok(header)
    );
    assert_eq!(header.encode(), [1, 0, 0, 0]);
}

#[test]
fn all_opcodes_round_trip_without_changing_fields() {
    for opcode in [Opcode::Read, Opcode::Write, Opcode::Set, Opcode::Clear] {
        let header = Header {
            opcode,
            index: 0x25,
            length: 12,
        };

        assert_eq!(
            Header::decode(&header.encode()),
            Ok(header)
        );
    }
}
