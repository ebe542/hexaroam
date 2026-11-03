use super::*;
use crate::{Access, HEADER_SIZE, Header, Opcode, encode_frame};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

static REGIONS: [RegisterRegion; 3] = [
    RegisterRegion {
        first_index: 1,
        entries: 1,
        bytes_per_entry: 1,
        access: Access::ReadOnly,
    },
    RegisterRegion {
        first_index: 3,
        entries: 1,
        bytes_per_entry: 1,
        access: Access::ReadWriteBits,
    },
    RegisterRegion {
        first_index: 0x20,
        entries: 18,
        bytes_per_entry: 4,
        access: Access::ReadWrite,
    },
];

fn checksum(data: &[u8]) -> u32 {
    SoftwareCrc32.checksum(data)
}

fn parser(kind: FrameKind) -> StreamParser<'static> {
    StreamParser::new(kind, &REGIONS).unwrap()
}

fn feed(parser: &mut StreamParser<'_>, bytes: &[u8]) {
    for &byte in bytes {
        parser.push(byte).unwrap();
    }
}

fn encoded(
    opcode: Opcode,
    index: u8,
    payload: &[u8],
    kind: FrameKind,
) -> ([u8; MAX_FRAME_SIZE], usize) {
    let mut output = [0; MAX_FRAME_SIZE];
    let length = encode_frame(
        Header {
            opcode,
            index,
            length: payload.len() as u16,
        },
        payload,
        kind,
        &mut output,
        &mut checksum,
    )
    .unwrap();

    (output, length)
}

// Deliberately supports invalid requested lengths and indices.
// A READ request always has exactly header + CRC bytes.
fn read_request(index: u8, length: u16) -> [u8; 8] {
    let mut bytes = [0; 8];
    bytes[..HEADER_SIZE].copy_from_slice(
        &Header {
            opcode: Opcode::Read,
            index,
            length,
        }
        .encode(),
    );

    let crc = checksum(&bytes[..HEADER_SIZE]);
    bytes[HEADER_SIZE..].copy_from_slice(&crc.to_le_bytes());
    bytes
}

fn raw_response(
    opcode: Opcode,
    index: u8,
    payload: &[u8],
) -> ([u8; MAX_FRAME_SIZE], usize) {
    let mut bytes = [0; MAX_FRAME_SIZE];
    let header = Header {
        opcode,
        index,
        length: payload.len() as u16,
    };

    bytes[..HEADER_SIZE].copy_from_slice(&header.encode());

    let crc_offset = HEADER_SIZE + payload.len();
    bytes[HEADER_SIZE..crc_offset].copy_from_slice(payload);

    let crc = checksum(&bytes[..crc_offset]);
    bytes[crc_offset..crc_offset + CRC_SIZE]
        .copy_from_slice(&crc.to_le_bytes());

    (bytes, crc_offset + CRC_SIZE)
}

#[test]
fn empty_parser_has_no_frame() {
    let mut parser = parser(FrameKind::Request);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn invalid_register_map_is_rejected_at_construction() {
    let invalid = [RegisterRegion {
        first_index: 255,
        entries: 2,
        bytes_per_entry: 1,
        access: Access::ReadOnly,
    }];

    assert!(matches!(
        StreamParser::new(FrameKind::Request, &invalid),
        Err(ParserError::InvalidMap),
    ));
}

#[test]
fn read_request_returns_header_without_payload() {
    let mut parser = parser(FrameKind::Request);
    feed(&mut parser, &read_request(1, 1));

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();
    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.index, 1);
    assert_eq!(parsed.frame.header.length, 1);
    assert!(parsed.frame.payload.is_empty());

    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn ack_response_returns_its_payload() {
    let mut parser = parser(FrameKind::Response);
    let (bytes, length) = encoded(
        Opcode::Ack,
        1,
        &[3],
        FrameKind::Response,
    );
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.opcode, Opcode::Ack);
    assert_eq!(parsed.frame.header.index, 1);
    assert_eq!(parsed.frame.payload, &[3]);
}

#[test]
fn fragmented_frame_is_not_returned_early() {
    let mut parser = parser(FrameKind::Request);
    let bytes = read_request(1, 1);

    for &byte in &bytes[..bytes.len() - 1] {
        parser.push(byte).unwrap();
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }

    parser
        .push(bytes[bytes.len() - 1])
        .unwrap();
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_some()
    );
}

#[test]
fn consecutive_frames_are_returned_once_each() {
    let mut parser = parser(FrameKind::Request);
    feed(&mut parser, &read_request(1, 1));
    feed(&mut parser, &read_request(3, 1));

    assert_eq!(
        parser
            .next_frame(&mut checksum)
            .unwrap()
            .frame
            .header
            .index,
        1,
    );
    assert_eq!(
        parser
            .next_frame(&mut checksum)
            .unwrap()
            .frame
            .header
            .index,
        3,
    );
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn noise_before_frame_is_skipped() {
    let mut parser = parser(FrameKind::Request);
    feed(&mut parser, &[0xFF, 0x80, 0]);
    feed(&mut parser, &read_request(1, 1));

    assert_eq!(
        parser
            .next_frame(&mut checksum)
            .unwrap()
            .frame
            .header
            .index,
        1,
    );
}

#[test]
fn opcode_bytes_inside_payload_do_not_restart_parser() {
    let mut parser = parser(FrameKind::Request);
    let payload = [1, 2, 3, 4];
    let (bytes, length) = encoded(
        Opcode::Write,
        0x20,
        &payload,
        FrameKind::Request,
    );
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();
    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(
        parsed.frame.header.opcode,
        Opcode::Write
    );
    assert_eq!(parsed.frame.payload, payload);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn impossible_write_header_is_rejected_without_crc_work() {
    let mut parser = parser(FrameKind::Request);
    // Unknown index and impossible length; no payload or CRC follows.
    feed(&mut parser, &[2, 0x80, 0xFF, 0xFF]);

    let mut calls = 0;
    {
        let mut counted = |data: &[u8]| {
            calls += 1;
            checksum(data)
        };
        assert!(
            parser
                .next_frame(&mut counted)
                .is_none()
        );
    }
    assert_eq!(calls, 0);

    feed(&mut parser, &read_request(1, 1));
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_some()
    );
}

#[test]
fn implausible_request_headers_are_rejected_without_crc_work() {
    for opcode in [Opcode::Read, Opcode::Write, Opcode::Set, Opcode::Clear] {
        for (index, length) in [
            (0xFF, 1_u16), // Unknown register.
            (1, 0),        // Empty access.
            (1, 256),      // Status has exactly one byte.
            (3, 2),        // Control has exactly one byte.
            (0x20, 3),     // Target values require four-byte alignment.
            (0x31, 8),     // Access crosses the target-region boundary.
        ] {
            let mut parser = parser(FrameKind::Request);
            let bytes = Header {
                opcode,
                index,
                length,
            }
            .encode();
            feed(&mut parser, &bytes);

            let mut calls = 0;
            {
                let mut counted = |data: &[u8]| {
                    calls += 1;
                    checksum(data)
                };
                assert!(
                    parser
                        .next_frame(&mut counted)
                        .is_none()
                );
            }
            assert_eq!(calls, 0);

            // Reject without waiting for a payload, CRC or timeout.
            feed(&mut parser, &read_request(1, 1));
            let parsed = parser
                .next_frame(&mut checksum)
                .unwrap();
            assert_eq!(parsed.validation, Ok(()));
            assert_eq!(parsed.frame.header.opcode, Opcode::Read);
            assert_eq!(parsed.frame.header.index, 1);
            assert_eq!(parsed.frame.header.length, 1);
            assert!(parsed.frame.payload.is_empty());
            assert!(
                parser
                    .next_frame(&mut checksum)
                    .is_none()
            );
        }
    }
}

#[test]
fn plausible_readonly_write_still_requires_crc_validation() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        let mut parser = parser(FrameKind::Request);
        let (bytes, length) = encoded(opcode, 1, &[0x55], FrameKind::Request);
        feed(&mut parser, &bytes[..length]);

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();
        assert_eq!(
            parsed.validation,
            Err(ValidationError::UnsupportedAccess),
        );
        assert_eq!(parsed.frame.header.opcode, opcode);
        assert_eq!(parsed.frame.header.index, 1);
        assert_eq!(parsed.frame.payload, &[0x55]);
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }
}

#[test]
fn crc_failure_is_skipped_and_next_frame_is_found() {
    let mut parser = parser(FrameKind::Request);
    let mut damaged = read_request(1, 1);
    damaged[7] ^= 0x80;

    feed(&mut parser, &damaged);
    feed(&mut parser, &read_request(3, 1));

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();
    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.index, 3);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn overlapping_candidates_can_reveal_multiple_buffered_frames() {
    let mut parser = parser(FrameKind::Request);
    let mut bytes = [0_u8; 24];

    // Plausible WRITE: 4-byte header + 16-byte payload + 4-byte CRC.
    bytes[..4].copy_from_slice(&[2, 0x20, 16, 0]);
    bytes[4..12].copy_from_slice(&read_request(1, 1));
    bytes[12..20].copy_from_slice(&read_request(3, 1));

    // Ensure the outer candidate fails CRC.
    let wrong_crc = checksum(&bytes[..20]) ^ 1;
    bytes[20..].copy_from_slice(&wrong_crc.to_le_bytes());

    feed(&mut parser, &bytes);

    assert_eq!(
        parser
            .next_frame(&mut checksum)
            .unwrap()
            .frame
            .header
            .index,
        1,
    );
    assert_eq!(
        parser
            .next_frame(&mut checksum)
            .unwrap()
            .frame
            .header
            .index,
        3,
    );
}

#[test]
fn timeout_discards_incomplete_candidate() {
    let mut parser = parser(FrameKind::Request);
    feed(&mut parser, &[2, 0x20, 72, 0, 0x55]);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );

    parser.on_timeout();

    feed(&mut parser, &read_request(1, 1));
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_some()
    );
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn timeout_after_draining_allows_new_reception() {
    let mut parser = parser(FrameKind::Request);
    feed(&mut parser, &read_request(1, 1));
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_some()
    );
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );

    parser.on_timeout();

    feed(&mut parser, &read_request(3, 1));
    assert_eq!(
        parser
            .next_frame(&mut checksum)
            .unwrap()
            .frame
            .header
            .index,
        3,
    );
}

#[test]
fn full_buffer_rejects_new_byte_without_overwriting_old_data() {
    let mut parser = parser(FrameKind::Request);
    feed(&mut parser, &read_request(1, 1));

    for _ in 8..MAX_FRAME_SIZE {
        parser.push(0xFF).unwrap();
    }

    assert_eq!(
        parser.push(0),
        Err(ParserError::BufferFull)
    );

    assert_eq!(
        parser
            .next_frame(&mut checksum)
            .unwrap()
            .frame
            .header
            .index,
        1,
    );
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );

    feed(&mut parser, &read_request(3, 1));
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_some()
    );
}

#[test]
fn maximum_frame_is_supported() {
    let map = [RegisterRegion {
        first_index: 0x10,
        entries: 1,
        bytes_per_entry: 512,
        access: Access::ReadWrite,
    }];
    let mut parser = StreamParser::new(FrameKind::Request, &map).unwrap();

    let payload = [0x55; 512];
    let (bytes, length) = encoded(
        Opcode::Write,
        0x10,
        &payload,
        FrameKind::Request,
    );

    assert_eq!(length, MAX_FRAME_SIZE);
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();
    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.payload, payload);
}

#[test]
fn response_parser_skips_request_opcodes() {
    let mut parser = parser(FrameKind::Response);

    for opcode in [1, 2, 3, 4] {
        feed(&mut parser, &[opcode, 0xFF, 0xFF, 0xFF]);
    }

    let (bytes, length) = encoded(
        Opcode::Ack,
        1,
        &[3],
        FrameKind::Response,
    );
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.opcode, Opcode::Ack);
    assert_eq!(parsed.frame.payload, &[3]);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn empty_ack_is_valid_independently_of_register_width() {
    let mut parser = parser(FrameKind::Response);
    let (bytes, length) = encoded(
        Opcode::Ack,
        0x20,
        &[],
        FrameKind::Response,
    );
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.opcode, Opcode::Ack);
    assert_eq!(parsed.frame.header.index, 0x20);
    assert_eq!(parsed.frame.header.length, 0);
    assert!(parsed.frame.payload.is_empty());
}

#[test]
fn nack_for_unknown_register_returns_error_byte() {
    let mut parser = parser(FrameKind::Response);
    let (bytes, length) = encoded(
        Opcode::Nack,
        0xFF,
        &[1],
        FrameKind::Response,
    );
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.opcode, Opcode::Nack);
    assert_eq!(parsed.frame.header.index, 0xFF);
    assert_eq!(parsed.frame.payload, &[1]);
}

#[test]
fn invalid_nack_lengths_are_skipped_before_valid_response() {
    for payload in [&[][..], &[0xFF, 0xFF][..]] {
        let mut parser = parser(FrameKind::Response);

        let (invalid, invalid_length) =
            raw_response(Opcode::Nack, 0xFF, payload);
        feed(&mut parser, &invalid[..invalid_length]);

        let (valid, valid_length) = encoded(
            Opcode::Ack,
            1,
            &[3],
            FrameKind::Response,
        );
        feed(&mut parser, &valid[..valid_length]);

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();

        assert_eq!(parsed.validation, Ok(()));
        assert_eq!(parsed.frame.header.opcode, Opcode::Ack);
        assert_eq!(parsed.frame.header.index, 1);
        assert_eq!(parsed.frame.payload, &[3]);
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }
}

#[test]
fn damaged_response_crc_is_skipped_before_valid_response() {
    for opcode in [Opcode::Ack, Opcode::Nack] {
        let mut parser = parser(FrameKind::Response);

        let (mut damaged, length) =
            encoded(opcode, 1, &[5], FrameKind::Response);
        damaged[length - 1] ^= 1;
        feed(&mut parser, &damaged[..length]);

        let (valid, valid_length) =
            encoded(Opcode::Ack, 3, &[], FrameKind::Response);
        feed(&mut parser, &valid[..valid_length]);

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();

        assert_eq!(parsed.validation, Ok(()));
        assert_eq!(parsed.frame.header.opcode, Opcode::Ack);
        assert_eq!(parsed.frame.header.index, 3);
        assert!(parsed.frame.payload.is_empty());
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }
}

#[test]
fn fragmented_responses_are_not_returned_early() {
    for (opcode, payload) in [
        (Opcode::Ack, &[][..]),
        (Opcode::Ack, &[3][..]),
        (Opcode::Nack, &[5][..]),
    ] {
        let mut parser = parser(FrameKind::Response);
        let (bytes, length) = encoded(opcode, 1, payload, FrameKind::Response);

        for &byte in &bytes[..length - 1] {
            parser.push(byte).unwrap();
            assert!(
                parser
                    .next_frame(&mut checksum)
                    .is_none()
            );
        }

        parser.push(bytes[length - 1]).unwrap();

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();

        assert_eq!(parsed.validation, Ok(()));
        assert_eq!(parsed.frame.header.opcode, opcode);
        assert_eq!(parsed.frame.payload, payload);
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }
}

#[test]
fn consecutive_ack_and_nack_responses_are_returned_once_each() {
    let mut parser = parser(FrameKind::Response);

    for (opcode, index, payload) in [
        (Opcode::Ack, 3, &[][..]),
        (Opcode::Nack, 0xFF, &[1][..]),
        (Opcode::Ack, 1, &[3][..]),
    ] {
        let (bytes, length) = encoded(
            opcode,
            index,
            payload,
            FrameKind::Response,
        );
        feed(&mut parser, &bytes[..length]);
    }

    for (opcode, index, payload) in [
        (Opcode::Ack, 3, &[][..]),
        (Opcode::Nack, 0xFF, &[1][..]),
        (Opcode::Ack, 1, &[3][..]),
    ] {
        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();

        assert_eq!(parsed.validation, Ok(()));
        assert_eq!(parsed.frame.header.opcode, opcode);
        assert_eq!(parsed.frame.header.index, index);
        assert_eq!(parsed.frame.payload, payload);
    }

    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn request_parser_skips_responses_before_valid_request() {
    for (opcode, payload) in
        [(Opcode::Ack, &[][..]), (Opcode::Nack, &[0xFF][..])]
    {
        let mut parser = parser(FrameKind::Request);

        let (bytes, length) = encoded(
            opcode,
            0xFF,
            payload,
            FrameKind::Response,
        );
        feed(&mut parser, &bytes[..length]);
        feed(&mut parser, &read_request(1, 1));

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();

        assert_eq!(parsed.validation, Ok(()));
        assert_eq!(parsed.frame.header.opcode, Opcode::Read);
        assert_eq!(parsed.frame.header.index, 1);
        assert!(parsed.frame.payload.is_empty());
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }
}

#[test]
fn invalid_register_accesses_are_delivered_after_crc_validation() {
    for (opcode, index, payload, error) in [
        (
            Opcode::Write,
            1,
            &[0x55][..],
            ValidationError::UnsupportedAccess,
        ),
        (
            Opcode::Set,
            1,
            &[0x55][..],
            ValidationError::UnsupportedAccess,
        ),
        (
            Opcode::Clear,
            1,
            &[0x55][..],
            ValidationError::UnsupportedAccess,
        ),
        (
            Opcode::Set,
            0x20,
            &[0; 4][..],
            ValidationError::UnsupportedAccess,
        ),
    ] {
        let mut parser = parser(FrameKind::Request);
        let (bytes, length) = encoded(
            opcode,
            index,
            payload,
            FrameKind::Request,
        );
        feed(&mut parser, &bytes[..length]);

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();

        assert_eq!(parsed.validation, Err(error));
        assert_eq!(parsed.frame.header.opcode, opcode);
        assert_eq!(parsed.frame.header.index, index);
        assert_eq!(
            parsed.frame.header.length,
            payload.len() as u16
        );
        assert_eq!(parsed.frame.payload, payload);

        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }
}

#[test]
fn invalid_write_access_waits_for_complete_frame() {
    let mut parser = parser(FrameKind::Request);
    let payload = [0x55];
    let (bytes, length) = encoded(
        Opcode::Write,
        1,
        &payload,
        FrameKind::Request,
    );

    for &byte in &bytes[..length - 1] {
        parser.push(byte).unwrap();
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );
    }

    parser.push(bytes[length - 1]).unwrap();

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(
        parsed.validation,
        Err(ValidationError::UnsupportedAccess),
    );
    assert_eq!(
        parsed.frame.header.opcode,
        Opcode::Write
    );
    assert_eq!(parsed.frame.header.index, 1);
    assert_eq!(parsed.frame.payload, payload);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn invalid_write_access_with_bad_crc_is_discarded() {
    let mut parser = parser(FrameKind::Request);
    let (mut bytes, length) = encoded(
        Opcode::Write,
        1,
        &[0x55],
        FrameKind::Request,
    );

    bytes[length - 1] ^= 1;
    feed(&mut parser, &bytes[..length]);

    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );

    feed(&mut parser, &read_request(3, 1));

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.opcode, Opcode::Read);
    assert_eq!(parsed.frame.header.index, 3);
    assert!(parsed.frame.payload.is_empty());
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}
