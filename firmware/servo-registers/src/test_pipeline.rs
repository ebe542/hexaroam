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
/// A modifying request must produce no response. READ responses are
/// encoded here and decoded separately by the calling test.
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
            length: 1,
        },
        &[],
        &mut response,
    )
    .expect("READ response");

    assert_eq!(length, 9);

    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Read,
            index: 1,
            length: 1,
        },
    );
    assert_eq!(
        frame.payload,
        &[STATUS_CALIBRATION_VALID]
    );
}

#[test]
fn set_control_has_no_response_and_changes_status() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut response = [0xA5; 12];

    assert_eq!(
        exchange(
            &mut service,
            Header {
                opcode: Opcode::Set,
                index: 3,
                length: 1,
            },
            &[OUTPUT_ENABLE],
            &mut response,
        ),
        None,
    );
    assert_eq!(response, [0xA5; 12]);

    let length = exchange(
        &mut service,
        Header {
            opcode: Opcode::Read,
            index: 1,
            length: 1,
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
        &[STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE],
    );
}

#[test]
fn unknown_read_returns_canonical_error_response() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut response = [0; 12];

    let length = exchange(
        &mut service,
        Header {
            opcode: Opcode::Read,
            index: 0xFF,
            length: 1,
        },
        &[],
        &mut response,
    )
    .expect("Error response");

    assert_eq!(length, 12);

    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid error response CRC");

    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Read,
            index: 2,
            length: 4,
        },
    );
    assert_eq!(
        frame.payload,
        &(ErrorCode::UnknownIndex as u32).to_le_bytes(),
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
/// These tests allow at most one READ response per call.
/// Modifying requests may precede it without producing a response.
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
                assert!(
                    response_length.is_none(),
                    "At most one READ response per call",
                );

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

    (handled, response_length)
}

fn read_byte(
    parser: &mut StreamParser<'_>,
    service: &mut RegisterService<FakeOutput>,
    index: u8,
) -> u8 {
    let (request, size) = request_bytes(Opcode::Read, index, 1, &[]);
    let mut response = [0; 12];

    let (handled, length) = feed(
        parser,
        service,
        &request[..size],
        &mut response,
    );

    assert_eq!(handled, 1);

    let frame = decode_frame(
        &response[..length.expect("READ response")],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Read,
            index,
            length: 1,
        },
    );
    assert_eq!(frame.payload.len(), 1);

    frame.payload[0]
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
            length: 1,
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
        &[STATUS_CALIBRATION_VALID]
    );

    let length = exchange(
        &mut service,
        Header {
            opcode: Opcode::Read,
            index: 2,
            length: 4,
        },
        &[],
        &mut response,
    )
    .expect("LastError response");

    let frame = decode_frame(
        &response[..length],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(
        frame.payload,
        &(ErrorCode::None as u32).to_le_bytes(),
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
        read_byte(&mut parser, &mut service, 1),
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
        read_byte(&mut parser, &mut service, 1),
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
        (1, None),
    );
    assert_eq!(
        read_byte(&mut parser, &mut service, 1),
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
    let (read, read_size) = request_bytes(Opcode::Read, 1, 1, &[]);

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

    let mut response = [0; 12];
    let (handled, length) = feed(
        &mut parser,
        &mut service,
        &stream[..size],
        &mut response,
    );

    assert_eq!(handled, 3);

    let frame = decode_frame(
        &response[..length.expect("Status response")],
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response CRC");

    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Read,
            index: 1,
            length: 1,
        },
    );

    // SET followed by CLR must leave output inactive.
    assert_eq!(
        frame.payload,
        &[STATUS_CALIBRATION_VALID]
    );
}

/// Writes consecutive targets through the complete request path.
fn wire_write_targets(
    parser: &mut StreamParser<'_>,
    service: &mut RegisterService<FakeOutput>,
    first: u8,
    values: &[i32],
) {
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

    assert_eq!(
        feed(
            parser,
            service,
            &request[..size],
            &mut response,
        ),
        (1, None),
    );
    assert_eq!(response, [0xA5; 80]);
}

/// Checks response header, payload, total length and CRC.
///
/// Requested and response indices may differ for canonical errors.
fn assert_wire_read(
    parser: &mut StreamParser<'_>,
    service: &mut RegisterService<FakeOutput>,
    requested_index: u8,
    requested_length: u16,
    response_index: u8,
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
            opcode: Opcode::Read,
            index: response_index,
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
                (1, None),
            );
            assert_eq!(response, [0xA5; 12]);
        }

        let mut values = [0_i32; 18];
        let mut expected = [0; 18 * 4];

        for (index, value) in values.iter_mut().enumerate() {
            *value = -8_500 + index as i32 * 1_000;
            expected[index * 4..index * 4 + 4]
                .copy_from_slice(&value.to_le_bytes());
        }

        wire_write_targets(&mut parser, &mut service, 0, &values);

        assert_wire_read(
            &mut parser,
            &mut service,
            0x20,
            72,
            0x20,
            &expected,
        );
        assert_wire_read(
            &mut parser,
            &mut service,
            2,
            4,
            2,
            &(ErrorCode::None as u32).to_le_bytes(),
        );

        let status = STATUS_CALIBRATION_VALID
            | if active { STATUS_OUTPUT_ACTIVE } else { 0 };

        assert_eq!(
            read_byte(&mut parser, &mut service, 1),
            status,
        );
    }
}

#[test]
fn partial_wire_write_preserves_other_targets() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    wire_write_targets(&mut parser, &mut service, 0, &[100; 18]);
    wire_write_targets(
        &mut parser,
        &mut service,
        4,
        &[-4_500, 1_234],
    );
    wire_write_targets(&mut parser, &mut service, 17, &[9_000]);

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
        0x20,
        &expected,
    );
}

#[test]
fn invalid_wire_batch_preserves_every_target() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    wire_write_targets(&mut parser, &mut service, 0, &[100; 18]);

    // The first two values are valid, but the final value is not.
    // No part of this CRC-valid request may be accepted.
    wire_write_targets(
        &mut parser,
        &mut service,
        4,
        &[-4_500, 1_234, 9_001],
    );

    assert_wire_read(
        &mut parser,
        &mut service,
        2,
        4,
        2,
        &(ErrorCode::InvalidValue as u32).to_le_bytes(),
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
        0x20,
        &expected,
    );

    // Successful readback must not clear the previous error.
    assert_wire_read(
        &mut parser,
        &mut service,
        2,
        4,
        2,
        &(ErrorCode::InvalidValue as u32).to_le_bytes(),
    );
    assert_eq!(
        read_byte(&mut parser, &mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn target_read_crossing_region_returns_wire_error() {
    let mut service = RegisterService::new(1, 18, FakeOutput::inactive(), true);
    let mut parser = StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)
        .expect("Valid register map");

    wire_write_targets(&mut parser, &mut service, 17, &[4_500]);

    // Servo 17 exists, but a second entry would exceed the region.
    assert_wire_read(
        &mut parser,
        &mut service,
        0x31,
        8,
        2,
        &(ErrorCode::OutOfBounds as u32).to_le_bytes(),
    );

    // The same parser remains usable after the rejected READ.
    assert_wire_read(
        &mut parser,
        &mut service,
        0x31,
        4,
        0x31,
        &4_500_i32.to_le_bytes(),
    );
}
