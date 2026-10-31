use super::*;
use crate::Opcode;
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

fn checksum(data: &[u8]) -> u32 {
    SoftwareCrc32.checksum(data)
}

fn header(opcode: Opcode, index: u8, length: u16) -> Header {
    Header {
        opcode,
        index,
        length,
    }
}

// Independent wire vectors calculated using Python's zlib.crc32.
const READ_REQUEST: [u8; 8] = [1, 1, 1, 0, 0x0F, 0xE3, 0x21, 0x81];

const WRITE_REQUEST: [u8; 12] = [
    2, 0x20, 4, 0, 0x6C, 0xEE, 0xFF, 0xFF, // -4500 centidegrees
    0x70, 0x04, 0x81, 0x1F,
];

const READ_RESPONSE: [u8; 9] = [1, 1, 1, 0, 3, 0x45, 0x82, 0x35, 0xDB];

#[test]
fn read_request_has_no_payload_despite_requested_length() {
    let expected = header(Opcode::Read, 1, 1);
    let mut output = [0; MAX_FRAME_SIZE];

    let length = encode_frame(
        expected,
        &[],
        FrameKind::Request,
        &mut output,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(&output[..length], &READ_REQUEST);

    let decoded = decode_frame(
        &READ_REQUEST,
        FrameKind::Request,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(decoded.header, expected);
    assert!(decoded.payload.is_empty());
}

#[test]
fn write_matches_signed_angle_wire_vector() {
    let expected = header(Opcode::Write, 0x20, 4);
    let payload = (-4_500_i32).to_le_bytes();
    let mut output = [0; MAX_FRAME_SIZE];

    let length = encode_frame(
        expected,
        &payload,
        FrameKind::Request,
        &mut output,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(&output[..length], &WRITE_REQUEST);

    let decoded = decode_frame(
        &WRITE_REQUEST,
        FrameKind::Request,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(decoded.header, expected);
    assert_eq!(decoded.payload, payload);
}

#[test]
fn read_response_contains_the_requested_payload() {
    let expected = header(Opcode::Read, 1, 1);
    let mut output = [0; MAX_FRAME_SIZE];

    let length = encode_frame(
        expected,
        &[3],
        FrameKind::Response,
        &mut output,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(&output[..length], &READ_RESPONSE);

    let decoded = decode_frame(
        &READ_RESPONSE,
        FrameKind::Response,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(decoded.header, expected);
    assert_eq!(decoded.payload, &[3]);
}

#[test]
fn write_set_and_clear_requests_round_trip() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        let expected = header(opcode, 3, 1);
        let mut output = [0; MAX_FRAME_SIZE];

        let length = encode_frame(
            expected,
            &[1],
            FrameKind::Request,
            &mut output,
            &mut checksum,
        )
        .unwrap();

        let decoded = decode_frame(
            &output[..length],
            FrameKind::Request,
            &mut checksum,
        )
        .unwrap();

        assert_eq!(decoded.header, expected);
        assert_eq!(decoded.payload, &[1]);

        assert_eq!(
            decode_frame(
                &output[..length],
                FrameKind::Response,
                &mut checksum,
            ),
            Err(FrameError::InvalidResponseOpcode),
        );
    }
}

#[test]
fn only_read_can_be_encoded_as_a_response() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        let mut output = [0xA5; MAX_FRAME_SIZE];

        assert_eq!(
            encode_frame(
                header(opcode, 3, 1),
                &[1],
                FrameKind::Response,
                &mut output,
                &mut checksum,
            ),
            Err(FrameError::InvalidResponseOpcode),
        );
        assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);
    }
}

#[test]
fn truncated_and_trailing_bytes_are_rejected() {
    for length in 0..READ_REQUEST.len() {
        assert_eq!(
            decode_frame(
                &READ_REQUEST[..length],
                FrameKind::Request,
                &mut checksum,
            ),
            Err(FrameError::InvalidLength),
        );
    }

    let mut extra = [0; READ_REQUEST.len() + 1];
    extra[..READ_REQUEST.len()].copy_from_slice(&READ_REQUEST);

    assert_eq!(
        decode_frame(
            &extra,
            FrameKind::Request,
            &mut checksum
        ),
        Err(FrameError::InvalidLength),
    );

    assert_eq!(
        decode_frame(
            &READ_RESPONSE,
            FrameKind::Request,
            &mut checksum,
        ),
        Err(FrameError::InvalidLength),
    );

    assert_eq!(
        decode_frame(
            &READ_REQUEST,
            FrameKind::Response,
            &mut checksum,
        ),
        Err(FrameError::InvalidLength),
    );
}

#[test]
fn corruption_in_header_payload_and_crc_is_detected() {
    // Change index, payload or CRC without changing frame length.
    for position in [1, 4, WRITE_REQUEST.len() - 1] {
        let mut damaged = WRITE_REQUEST;
        damaged[position] ^= 1;

        assert_eq!(
            decode_frame(
                &damaged,
                FrameKind::Request,
                &mut checksum,
            ),
            Err(FrameError::CrcMismatch),
        );
    }
}

#[test]
fn unknown_opcode_is_rejected_before_crc_validation() {
    let mut damaged = READ_REQUEST;
    damaged[0] = 0xFF;

    assert_eq!(
        decode_frame(
            &damaged,
            FrameKind::Request,
            &mut checksum,
        ),
        Err(FrameError::Header(
            HeaderError::UnknownOpcode(0xFF)
        )),
    );
}

#[test]
fn inconsistent_payload_lengths_do_not_modify_output() {
    for (opcode, length, payload) in [
        (Opcode::Read, 1, &[1][..]),
        (Opcode::Write, 2, &[1][..]),
        (Opcode::Set, 1, &[][..]),
        (Opcode::Clear, 1, &[1, 2][..]),
    ] {
        let mut output = [0xA5; MAX_FRAME_SIZE];

        assert_eq!(
            encode_frame(
                header(opcode, 3, length),
                payload,
                FrameKind::Request,
                &mut output,
                &mut checksum,
            ),
            Err(FrameError::InvalidLength),
        );
        assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);
    }
}

#[test]
fn oversized_declared_payloads_are_rejected() {
    let oversized = header(
        Opcode::Read,
        1,
        (MAX_PAYLOAD_SIZE + 1) as u16,
    );
    let mut output = [0xA5; MAX_FRAME_SIZE];

    assert_eq!(
        encode_frame(
            oversized,
            &[],
            FrameKind::Request,
            &mut output,
            &mut checksum,
        ),
        Err(FrameError::PayloadTooLarge),
    );
    assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);

    let mut input = [0; HEADER_SIZE + CRC_SIZE];
    input[..HEADER_SIZE].copy_from_slice(&oversized.encode());

    assert_eq!(
        decode_frame(
            &input,
            FrameKind::Request,
            &mut checksum
        ),
        Err(FrameError::PayloadTooLarge),
    );
}

#[test]
fn insufficient_output_space_preserves_buffer() {
    let mut output = [0xA5; READ_REQUEST.len() - 1];

    assert_eq!(
        encode_frame(
            header(Opcode::Read, 1, 1),
            &[],
            FrameKind::Request,
            &mut output,
            &mut checksum,
        ),
        Err(FrameError::BufferTooSmall),
    );
    assert_eq!(output, [0xA5; READ_REQUEST.len() - 1]);
}

#[test]
fn maximum_payload_fits_exactly_and_round_trips() {
    // This checks framing capacity, not calibration/register semantics.
    let expected = header(
        Opcode::Write,
        0x10,
        MAX_PAYLOAD_SIZE as u16,
    );
    let payload = [0x5A; MAX_PAYLOAD_SIZE];
    let mut output = [0; MAX_FRAME_SIZE];

    let length = encode_frame(
        expected,
        &payload,
        FrameKind::Request,
        &mut output,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(length, MAX_FRAME_SIZE);

    let decoded = decode_frame(
        &output,
        FrameKind::Request,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(decoded.header, expected);
    assert_eq!(decoded.payload, payload);
}
