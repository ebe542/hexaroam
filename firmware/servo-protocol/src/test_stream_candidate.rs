use super::*;

const CALIBRATION_BYTES: usize = 368;

static LARGE_REGIONS: [RegisterRegion; 3] = [
    RegisterRegion {
        first_index: 1,
        entries: 1,
        bytes_per_entry: 4,
        access: Access::ReadOnly,
    },
    RegisterRegion {
        first_index: 3,
        entries: 1,
        bytes_per_entry: 1,
        access: Access::ReadWriteBits,
    },
    RegisterRegion {
        first_index: 0x10,
        entries: 1,
        bytes_per_entry: CALIBRATION_BYTES as u16,
        access: Access::ReadWrite,
    },
];

fn large_parser() -> StreamParser<'static> {
    StreamParser::new(FrameKind::Request, &LARGE_REGIONS).unwrap()
}

#[test]
fn fragmented_large_frame_keeps_header_and_payload_until_complete() {
    let mut parser = large_parser();
    let mut payload = [0; CALIBRATION_BYTES];
    let pattern = [1, 2, 3, 4, 0x80, 0x81, 0xFF];

    for (index, byte) in payload.iter_mut().enumerate() {
        *byte = pattern[index % pattern.len()];
    }

    let (bytes, length) = encoded(
        Opcode::Write,
        0x10,
        &payload,
        FrameKind::Request,
    );

    let mut crc_calls = 0;

    for &byte in &bytes[..length - 1] {
        parser.push(byte).unwrap();

        // Repeated polling without new input must preserve the candidate.
        for _ in 0..3 {
            let mut counted = |data: &[u8]| {
                crc_calls += 1;
                checksum(data)
            };

            assert!(
                parser
                    .next_frame(&mut counted)
                    .is_none()
            );
        }
    }

    assert_eq!(crc_calls, 0);

    parser.push(bytes[length - 1]).unwrap();

    {
        let mut counted = |data: &[u8]| {
            crc_calls += 1;
            checksum(data)
        };

        let parsed = parser.next_frame(&mut counted).unwrap();

        assert_eq!(parsed.validation, Ok(()));
        assert_eq!(
            parsed.frame.header,
            Header {
                opcode: Opcode::Write,
                index: 0x10,
                length: CALIBRATION_BYTES as u16,
            },
        );
        assert_eq!(parsed.frame.payload, payload);
    }

    assert_eq!(crc_calls, 1);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn completed_large_frame_does_not_affect_following_short_request() {
    let mut parser = large_parser();
    let payload = [0xFF; CALIBRATION_BYTES];
    let (bytes, length) = encoded(
        Opcode::Write,
        0x10,
        &payload,
        FrameKind::Request,
    );

    feed(&mut parser, &bytes[..length]);

    {
        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();
        assert_eq!(
            parsed.frame.header.opcode,
            Opcode::Write
        );
        assert_eq!(parsed.frame.payload, payload);
    }

    // push() must retire the previous frame and its candidate state.
    feed(&mut parser, &read_request(1, 4));

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(
        parsed.frame.header,
        Header {
            opcode: Opcode::Read,
            index: 1,
            length: 4,
        },
    );
    assert!(parsed.frame.payload.is_empty());
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn timeout_discards_large_candidate_before_short_request() {
    let mut parser = large_parser();
    let payload = [0xFF; CALIBRATION_BYTES];
    let (bytes, _) = encoded(
        Opcode::Write,
        0x10,
        &payload,
        FrameKind::Request,
    );

    feed(&mut parser, &bytes[..HEADER_SIZE + 17]);
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );

    parser.on_timeout();
    feed(&mut parser, &read_request(1, 4));

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.opcode, Opcode::Read);
    assert_eq!(parsed.frame.header.index, 1);
    assert_eq!(parsed.frame.header.length, 4);
    assert!(parsed.frame.payload.is_empty());
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn crc_failure_discards_large_candidate_before_short_request() {
    let mut parser = large_parser();
    let payload = [0xFF; CALIBRATION_BYTES];
    let (mut bytes, length) = encoded(
        Opcode::Write,
        0x10,
        &payload,
        FrameKind::Request,
    );

    // Use non-opcode bytes as the incorrect CRC so recovery here tests
    // candidate reset rather than an unrelated false-header timeout.
    assert_ne!(
        &bytes[length - CRC_SIZE..length],
        &[0xFF; CRC_SIZE]
    );
    bytes[length - CRC_SIZE..length].fill(0xFF);

    feed(&mut parser, &bytes[..length]);
    feed(&mut parser, &read_request(1, 4));

    let parsed = parser
        .next_frame(&mut checksum)
        .unwrap();

    assert_eq!(parsed.validation, Ok(()));
    assert_eq!(parsed.frame.header.opcode, Opcode::Read);
    assert_eq!(parsed.frame.header.index, 1);
    assert_eq!(parsed.frame.header.length, 4);
    assert!(parsed.frame.payload.is_empty());
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

#[test]
fn rejected_access_does_not_leak_into_next_candidate() {
    let mut parser = large_parser();
    let payload = [0xFF; CALIBRATION_BYTES];

    for (opcode, validation) in [
        (
            Opcode::Set,
            Err(ValidationError::UnsupportedAccess),
        ),
        (Opcode::Write, Ok(())),
    ] {
        let (bytes, length) = encoded(
            opcode,
            0x10,
            &payload,
            FrameKind::Request,
        );

        feed(&mut parser, &bytes[..HEADER_SIZE]);
        assert!(
            parser
                .next_frame(&mut checksum)
                .is_none()
        );

        feed(&mut parser, &bytes[HEADER_SIZE..length]);

        let parsed = parser
            .next_frame(&mut checksum)
            .unwrap();

        assert_eq!(parsed.validation, validation);
        assert_eq!(parsed.frame.header.opcode, opcode);
        assert_eq!(parsed.frame.header.index, 0x10);
        assert_eq!(parsed.frame.payload, payload);
    }

    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}
