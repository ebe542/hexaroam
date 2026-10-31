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
fn read_response_returns_its_payload() {
    let mut parser = parser(FrameKind::Response);
    let (bytes, length) = encoded(
        Opcode::Read,
        1,
        &[3],
        FrameKind::Response,
    );
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();
    assert_eq!(parsed.validation, Ok(()));
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
fn crc_valid_read_with_unknown_index_is_delivered_as_error() {
    let mut parser = parser(FrameKind::Request);
    feed(&mut parser, &read_request(0xFF, 4));

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();
    assert_eq!(
        parsed.validation,
        Err(ValidationError::UnknownIndex),
    );
    assert_eq!(parsed.frame.header.index, 0xFF);
    assert!(parsed.frame.payload.is_empty());
}

#[test]
fn invalid_read_lengths_still_require_only_eight_bytes() {
    for (length, error) in [
        (0, ValidationError::InvalidLength),
        (2, ValidationError::OutOfBounds),
        (u16::MAX, ValidationError::OutOfBounds),
    ] {
        let mut parser = parser(FrameKind::Request);
        feed(&mut parser, &read_request(1, length));

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();
        assert_eq!(parsed.validation, Err(error));
        assert_eq!(parsed.frame.header.length, length);
        assert!(parsed.frame.payload.is_empty());
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
fn response_parser_skips_non_read_candidates() {
    let mut parser = parser(FrameKind::Response);
    feed(&mut parser, &[2, 0xFF, 0xFF, 0xFF]);

    let (bytes, length) = encoded(
        Opcode::Read,
        1,
        &[3],
        FrameKind::Response,
    );
    feed(&mut parser, &bytes[..length]);

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();
    assert_eq!(parsed.frame.header.opcode, Opcode::Read);
    assert_eq!(parsed.frame.payload, &[3]);
}
