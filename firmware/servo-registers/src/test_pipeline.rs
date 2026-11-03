//! Host-side tests connecting framing, register handling and responses.

use crate::{
    ErrorCode, OUTPUT_ENABLE, REGISTER_REGIONS, RegisterService,
    STATUS_CALIBRATION_VALID, STATUS_OUTPUT_ACTIVE, test_support::FakeOutput,
};
use hexaroam_servo_protocol::{
    FrameKind, Header, MAX_FRAME_SIZE, Opcode, StreamParser, decode_frame,
    encode_frame,
};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

fn checksum(data: &[u8]) -> u32 {
    SoftwareCrc32.checksum(data)
}

/// Sends one request byte by byte through the real parser and service.
///
/// Use this helper for plausible requests that must reach the service.
/// Their ACK or NACK is decoded separately by the calling test.
/// Tests for discarded headers use feed() to assert no service call.
fn exchange(
    service: &mut RegisterService<FakeOutput>,
    header: Header,
    payload: &[u8],
    response: &mut [u8],
) -> Option<usize> {
    let mut request = [0; MAX_FRAME_SIZE];
    let request_length = encode_frame(
        header,
        payload,
        FrameKind::Request,
        &mut request,
        &mut checksum,
    )
    .expect("Encode request");

    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    let mut response_payload = [0; 18 * 4];
    let mut handled = false;
    let mut response_length = None;

    for (position, &byte) in request[..request_length]
        .iter()
        .enumerate()
    {
        parser
            .push(byte)
            .expect("Parser capacity");

        while let Some(parsed) = parser.next_frame(&mut checksum) {
            assert!(!handled, "Exactly one request expected");
            assert_eq!(
                position + 1,
                request_length,
                "Do not handle an incomplete request",
            );
            handled = true;

            if let Some(frame) = service
                .handle(parsed, &mut response_payload)
                .expect("Handle request")
            {
                response_length = Some(
                    encode_frame(
                        frame.header,
                        frame.payload,
                        FrameKind::Response,
                        response,
                        &mut checksum,
                    )
                    .expect("Encode response"),
                );
            }
        }
    }

    assert!(
        handled,
        "Complete request must reach the service"
    );
    response_length
}

#[test]
fn status_read_returns_crc_checked_response() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut response = [0; 12];

    let length = exchange(
        &mut service,
        Header {
            opcode: Opcode::Read,
            index: 1,
            length: 4,
        },
        &[],
        &mut response,
    )
    .expect("READ response");

    assert_eq!(length, 12);

    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Ack,
            index: 1,
            length: 4,
        },
    );
    assert_eq!(
        frame.payload,
        &STATUS_CALIBRATION_VALID.to_le_bytes()
    );
}

#[test]
fn set_control_returns_ack_and_changes_status() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut response = [0xA5; 12];

    let length = exchange(
        &mut service,
        Header {
            opcode: Opcode::Set,
            index: 3,
            length: 1,
        },
        &[OUTPUT_ENABLE],
        &mut response,
    )
    .expect("SET response");
    assert_eq!(length, 8);
    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid ACK CRC");
    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Ack,
            index: 3,
            length: 0,
        }
    );
    assert!(frame.payload.is_empty());
    assert_eq!(&response[length..], &[0xA5; 4]);

    let length = exchange(
        &mut service,
        Header {
            opcode: Opcode::Read,
            index: 1,
            length: 4,
        },
        &[],
        &mut response,
    )
    .expect("Status response");

    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(frame.header.index, 1);
    assert_eq!(
        frame.payload,
        &(STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE).to_le_bytes(),
    );
}

#[test]
fn unknown_read_is_discarded_without_response() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");
    let (request, size) = request_bytes(Opcode::Read, 0xFF, 1, &[]);
    let mut response = [0xA5; 12];

    // Even a CRC-valid frame with an unknown index is discarded by the
    // parser rather than reaching the register service for a NACK.
    assert_eq!(
        feed(
            &mut parser,
            &mut service,
            &request[..size],
            &mut response,
        ),
        (0, None),
    );
    assert_eq!(response, [0xA5; 12]);

    // Recover with the same parser, without a timeout or replacement.
    assert_eq!(
        read_scalar(&mut parser, &mut service, 3, 1),
        0
    );
    assert_eq!(
        read_scalar(&mut parser, &mut service, 1, 4),
        STATUS_CALIBRATION_VALID,
    );
}

/// Encodes a request for byte-stream tests.
fn request_bytes(
    opcode: Opcode,
    index: u8,
    length: u16,
    payload: &[u8],
) -> ([u8; MAX_FRAME_SIZE], usize) {
    let mut bytes = [0; MAX_FRAME_SIZE];
    let size = encode_frame(
        Header {
            opcode,
            index,
            length,
        },
        payload,
        FrameKind::Request,
        &mut bytes,
        &mut checksum,
    )
    .expect("Encode request");

    (bytes, size)
}

/// Feeds bytes into an existing parser and handles complete requests.
///
/// Multiple responses are appended in request order without overwriting
/// previously encoded frames.
fn feed(
    parser: &mut StreamParser<'_>,
    service: &mut RegisterService<FakeOutput>,
    bytes: &[u8],
    response: &mut [u8],
) -> (usize, Option<usize>) {
    let mut handled = 0;
    let mut response_length = None;
    let mut response_payload = [0; 18 * 4];

    for &byte in bytes {
        parser
            .push(byte)
            .expect("Parser capacity");

        while let Some(parsed) = parser.next_frame(&mut checksum) {
            handled += 1;

            if let Some(frame) = service
                .handle(parsed, &mut response_payload)
                .expect("Handle request")
            {
                let offset = response_length.unwrap_or(0);
                let length = encode_frame(
                    frame.header,
                    frame.payload,
                    FrameKind::Response,
                    &mut response[offset..],
                    &mut checksum,
                )
                .expect("Encode response");
                response_length = Some(offset + length);
            }
        }
    }

    (handled, response_length)
}

fn read_scalar(
    parser: &mut StreamParser<'_>,
    service: &mut RegisterService<FakeOutput>,
    index: u8,
    width: u16,
) -> u32 {
    assert!((1..=4).contains(&width));
    let (request, size) = request_bytes(Opcode::Read, index, width, &[]);
    let mut response = [0; 12];
    let (handled, length) = feed(
        parser,
        service,
        &request[..size],
        &mut response,
    );
    assert_eq!(handled, 1);
    let length = length.expect("READ response");
    assert_eq!(length, 8 + usize::from(width));
    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");
    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Ack,
            index,
            length: width,
        }
    );
    assert_eq!(frame.payload.len(), usize::from(width));
    let mut bytes = [0; 4];
    bytes[..usize::from(width)].copy_from_slice(frame.payload);
    u32::from_le_bytes(bytes)
}

#[test]
fn corrupted_crc_does_not_reach_service() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");
    let (mut request, size) =
        request_bytes(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]);
    let mut response = [0xA5; 12];

    // Change only the CRC; the command itself would enable output.
    request[size - 1] ^= 0x80;

    assert_eq!(
        feed(
            &mut parser,
            &mut service,
            &request[..size],
            &mut response,
        ),
        (0, None),
    );
    assert_eq!(response, [0xA5; 12]);

    // Inspect service state independently of parser resynchronization.
    let length = exchange(
        &mut service,
        Header {
            opcode: Opcode::Read,
            index: 1,
            length: 4,
        },
        &[],
        &mut response,
    )
    .expect("Status response");

    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(
        frame.payload,
        &STATUS_CALIBRATION_VALID.to_le_bytes()
    );
}

#[test]
fn valid_request_is_recognized_after_corrupted_frame() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");
    let (mut request, size) =
        request_bytes(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]);
    let mut response = [0; 12];

    request[size - 1] ^= 0x80;

    assert_eq!(
        feed(
            &mut parser,
            &mut service,
            &request[..size],
            &mut response,
        ),
        (0, None),
    );

    // No timeout or parser replacement between the two requests.
    assert_eq!(
        read_scalar(&mut parser, &mut service, 1, 4),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn timeout_discards_incomplete_request() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");
    let (request, size) = request_bytes(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]);
    let mut response = [0xA5; 12];

    // The final CRC byte is missing: nothing may be executed.
    assert_eq!(
        feed(
            &mut parser,
            &mut service,
            &request[..size - 1],
            &mut response,
        ),
        (0, None),
    );
    assert_eq!(response, [0xA5; 12]);

    // Transport timing will trigger this event in the firmware.
    parser.on_timeout();

    assert_eq!(
        read_scalar(&mut parser, &mut service, 1, 4),
        STATUS_CALIBRATION_VALID,
    );

    // A complete command must work after recovery.
    assert_eq!(
        feed(
            &mut parser,
            &mut service,
            &request[..size],
            &mut response,
        ),
        (1, Some(8)),
    );
    let ack = decode_frame(
        &response[..8],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid recovery ACK");
    assert_eq!(
        ack.header,
        Header {
            opcode: Opcode::Ack,
            index: 3,
            length: 0,
        }
    );
    assert!(ack.payload.is_empty());
    assert_eq!(
        read_scalar(&mut parser, &mut service, 1, 4),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
    );
}

#[test]
fn consecutive_requests_are_processed_in_order() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    let (enable, enable_size) =
        request_bytes(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]);
    let (disable, disable_size) =
        request_bytes(Opcode::Clear, 3, 1, &[OUTPUT_ENABLE]);
    let (read, read_size) = request_bytes(Opcode::Read, 1, 4, &[]);

    // Three complete frames without gaps or timeout events.
    let mut stream = [0; MAX_FRAME_SIZE];
    let mut size = 0;

    for bytes in [
        &enable[..enable_size],
        &disable[..disable_size],
        &read[..read_size],
    ] {
        stream[size..size + bytes.len()].copy_from_slice(bytes);
        size += bytes.len();
    }

    let mut response = [0; 28];
    let (handled, length) = feed(
        &mut parser,
        &mut service,
        &stream[..size],
        &mut response,
    );

    assert_eq!(handled, 3);
    assert_eq!(length, Some(28));

    let status_bytes = STATUS_CALIBRATION_VALID.to_le_bytes();
    for (start, end, index, payload) in [
        (0, 8, 3, &[][..]),
        (8, 16, 3, &[][..]),
        (16, 28, 1, &status_bytes[..]),
    ] {
        let frame = decode_frame(
            &response[start..end],
            FrameKind::Response,
            &mut checksum,
        )
        .expect("Valid ordered response CRC");
        assert_eq!(
            frame.header,
            Header {
                opcode: Opcode::Ack,
                index,
                length: payload.len() as u16,
            }
        );
        assert_eq!(frame.payload, payload);
    }
    assert!(
        parser
            .next_frame(&mut checksum)
            .is_none()
    );
}

/// Writes consecutive targets through the complete request path.
fn wire_write_targets(
    parser: &mut StreamParser<'_>,
    service: &mut RegisterService<FakeOutput>,
    first: u8,
    values: &[i32],
) -> Result<(), u8> {
    let mut payload = [0; 18 * 4];

    for (offset, value) in values.iter().enumerate() {
        payload[offset * 4..offset * 4 + 4]
            .copy_from_slice(&value.to_le_bytes());
    }

    let payload_length = values.len() * 4;
    let (request, size) = request_bytes(
        Opcode::Write,
        0x20 + first,
        payload_length as u16,
        &payload[..payload_length],
    );
    let mut response = [0xA5; 80];

    let (handled, length) = feed(
        parser,
        service,
        &request[..size],
        &mut response,
    );
    assert_eq!(handled, 1);
    let length = length.expect("Target-write response");
    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid target-write response CRC");
    assert_eq!(frame.header.index, 0x20 + first);
    let result = match frame.header.opcode {
        Opcode::Ack => {
            assert_eq!(frame.header.length, 0);
            assert!(frame.payload.is_empty());
            assert_eq!(length, 8);
            Ok(())
        }
        Opcode::Nack => {
            assert_eq!(frame.header.length, 1);
            assert_eq!(frame.payload.len(), 1);
            assert_eq!(length, 9);
            Err(frame.payload[0])
        }
        other => panic!("Unexpected response opcode: {other:?}"),
    };
    assert!(
        response[length..]
            .iter()
            .all(|&byte| byte == 0xA5)
    );
    result
}

/// Checks response header, payload, total length and CRC.
///
/// ACK and NACK both preserve the requested register index.
fn assert_wire_read(
    parser: &mut StreamParser<'_>,
    service: &mut RegisterService<FakeOutput>,
    requested_index: u8,
    requested_length: u16,
    response_opcode: Opcode,
    expected_payload: &[u8],
) {
    let (request, size) = request_bytes(
        Opcode::Read,
        requested_index,
        requested_length,
        &[],
    );
    let mut response = [0; 80];

    let (handled, length) = feed(
        parser,
        service,
        &request[..size],
        &mut response,
    );

    assert_eq!(handled, 1);
    let length = length.expect("READ response");
    assert_eq!(length, 4 + expected_payload.len() + 4);

    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(
        frame.header,
        Header {
            opcode: response_opcode,
            index: requested_index,
            length: expected_payload.len() as u16,
        },
    );
    assert_eq!(frame.payload, expected_payload);
}

#[test]
fn all_targets_round_trip_through_wire_frames() {
    for active in [false, true] {
        let mut service =
            RegisterService::new(1, 18, FakeOutput::inactive(), true);
        let mut parser =
            StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
                .expect("Valid register map");

        if active {
            let (request, size) =
                request_bytes(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]);
            let mut response = [0xA5; 12];

            assert_eq!(
                feed(
                    &mut parser,
                    &mut service,
                    &request[..size],
                    &mut response,
                ),
                (1, Some(8)),
            );
            let ack = decode_frame(
                &response[..8],
                FrameKind::Response,
                &mut checksum,
            )
            .expect("Valid activation ACK");
            assert_eq!(
                ack.header,
                Header {
                    opcode: Opcode::Ack,
                    index: 3,
                    length: 0,
                }
            );
            assert!(ack.payload.is_empty());
            assert_eq!(&response[8..], &[0xA5; 4]);
        }

        let mut values = [0_i32; 18];
        let mut expected = [0; 18 * 4];

        for (index, value) in values.iter_mut().enumerate() {
            *value = -8_500 + index as i32 * 1_000;
            expected[index * 4..index * 4 + 4]
                .copy_from_slice(&value.to_le_bytes());
        }

        wire_write_targets(&mut parser, &mut service, 0, &values).unwrap();

        assert_wire_read(
            &mut parser,
            &mut service,
            0x20,
            72,
            Opcode::Ack,
            &expected,
        );

        let status = STATUS_CALIBRATION_VALID
            | if active { STATUS_OUTPUT_ACTIVE } else { 0 };

        assert_eq!(
            read_scalar(&mut parser, &mut service, 1, 4),
            status,
        );
    }
}

#[test]
fn partial_wire_write_preserves_other_targets() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    wire_write_targets(&mut parser, &mut service, 0, &[100; 18]).unwrap();
    wire_write_targets(
        &mut parser,
        &mut service,
        4,
        &[-4_500, 1_234],
    )
    .unwrap();
    wire_write_targets(&mut parser, &mut service, 17, &[9_000]).unwrap();

    let mut values = [100_i32; 18];
    values[4] = -4_500;
    values[5] = 1_234;
    values[17] = 9_000;

    let mut expected = [0; 18 * 4];

    for (index, value) in values.iter().enumerate() {
        expected[index * 4..index * 4 + 4]
            .copy_from_slice(&value.to_le_bytes());
    }

    assert_wire_read(
        &mut parser,
        &mut service,
        0x20,
        72,
        Opcode::Ack,
        &expected,
    );
}

#[test]
fn invalid_wire_batch_preserves_every_target() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    wire_write_targets(&mut parser, &mut service, 0, &[100; 18]).unwrap();

    // The first two values are valid, but the final value is not.
    // No part of this CRC-valid request may be accepted.
    assert_eq!(
        wire_write_targets(
            &mut parser,
            &mut service,
            4,
            &[-4_500, 1_234, 9_001],
        ),
        Err(ErrorCode::InvalidValue as u8),
    );

    let mut expected = [0; 18 * 4];

    for bytes in expected.chunks_exact_mut(4) {
        bytes.copy_from_slice(&100_i32.to_le_bytes());
    }

    assert_wire_read(
        &mut parser,
        &mut service,
        0x20,
        72,
        Opcode::Ack,
        &expected,
    );

    assert_eq!(
        read_scalar(&mut parser, &mut service, 1, 4),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn target_read_crossing_region_is_discarded_without_response() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    wire_write_targets(&mut parser, &mut service, 17, &[4_500]).unwrap();

    // Servo 17 exists, but a second entry would exceed the region.
    // The impossible range is rejected before any service call.
    let (request, size) = request_bytes(Opcode::Read, 0x31, 8, &[]);
    let mut response = [0xA5; 12];
    assert_eq!(
        feed(
            &mut parser,
            &mut service,
            &request[..size],
            &mut response,
        ),
        (0, None),
    );
    assert_eq!(response, [0xA5; 12]);

    // The same parser remains usable and the stored target is unchanged.
    assert_wire_read(
        &mut parser,
        &mut service,
        0x31,
        4,
        Opcode::Ack,
        &4_500_i32.to_le_bytes(),
    );
}
