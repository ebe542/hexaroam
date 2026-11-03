extern crate std;
use super::*;
use crate::Opcode;
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};
use std::vec::Vec;

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

const ACK_RESPONSE: [u8; 9] = [0x80, 1, 1, 0, 3, 0x67, 0x1B, 0xB3, 0x57];

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
fn ack_response_contains_the_requested_payload() {
    let expected = header(Opcode::Ack, 1, 1);
    let mut output = [0; MAX_FRAME_SIZE];

    let length = encode_frame(
        expected,
        &[3],
        FrameKind::Response,
        &mut output,
        &mut checksum,
    )
    .unwrap();

    assert_eq!(&output[..length], &ACK_RESPONSE);

    let decoded = decode_frame(
        &ACK_RESPONSE,
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
fn request_opcodes_cannot_be_encoded_as_responses() {
    for opcode in [Opcode::Read, Opcode::Write, Opcode::Set, Opcode::Clear] {
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
            &ACK_RESPONSE,
            FrameKind::Request,
            &mut checksum,
        ),
        Err(FrameError::InvalidRequestOpcode),
    );

    assert_eq!(
        decode_frame(
            &READ_REQUEST,
            FrameKind::Response,
            &mut checksum,
        ),
        Err(FrameError::InvalidResponseOpcode),
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

fn wire_frame(header: Header, payload: &[u8]) -> Vec<u8> {
    let mut bytes = header.encode().to_vec();
    bytes.extend_from_slice(payload);
    let crc = checksum(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes
}

#[test]
fn response_opcodes_are_rejected_as_requests() {
    for (opcode, payload) in [(Opcode::Ack, &[][..]), (Opcode::Nack, &[5][..])]
    {
        let header = header(opcode, 3, payload.len() as u16);
        let mut output = [0xA5; MAX_FRAME_SIZE];

        assert_eq!(
            encode_frame(
                header,
                payload,
                FrameKind::Request,
                &mut output,
                &mut checksum,
            ),
            Err(FrameError::InvalidRequestOpcode),
        );
        assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);

        assert_eq!(
            decode_frame(
                &wire_frame(header, payload),
                FrameKind::Request,
                &mut checksum,
            ),
            Err(FrameError::InvalidRequestOpcode),
        );
    }
}

#[test]
fn ack_payload_lengths_from_zero_to_capacity_round_trip() {
    let payload = [0x5A; MAX_PAYLOAD_SIZE];

    for length in 0..=MAX_PAYLOAD_SIZE {
        let expected = header(Opcode::Ack, 0x20, length as u16);
        let mut output = [0; MAX_FRAME_SIZE];

        let written = encode_frame(
            expected,
            &payload[..length],
            FrameKind::Response,
            &mut output,
            &mut checksum,
        )
        .unwrap();

        assert_eq!(written, HEADER_SIZE + length + CRC_SIZE);

        let decoded = decode_frame(
            &output[..written],
            FrameKind::Response,
            &mut checksum,
        )
        .unwrap();

        assert_eq!(decoded.header, expected);
        assert_eq!(decoded.payload, &payload[..length]);
    }
}

#[test]
fn nack_carries_exactly_one_error_byte() {
    let expected = header(Opcode::Nack, 0x10, 1);

    for error in 0_u8..=u8::MAX {
        // Error-code semantics belong to the register layer.
        let mut output = [0; HEADER_SIZE + 1 + CRC_SIZE];

        let written = encode_frame(
            expected,
            &[error],
            FrameKind::Response,
            &mut output,
            &mut checksum,
        )
        .unwrap();

        assert_eq!(written, output.len());

        let decoded = decode_frame(
            &output,
            FrameKind::Response,
            &mut checksum,
        )
        .unwrap();

        assert_eq!(decoded.header, expected);
        assert_eq!(decoded.payload, &[error]);
    }
}

#[test]
fn nack_rejects_other_declared_lengths_without_modifying_output() {
    let payload = [0; MAX_PAYLOAD_SIZE];

    for length in 0..=MAX_PAYLOAD_SIZE {
        if length == 1 {
            continue;
        }

        let header = header(Opcode::Nack, 0x10, length as u16);
        let mut output = [0xA5; MAX_FRAME_SIZE];

        assert_eq!(
            encode_frame(
                header,
                &payload[..length],
                FrameKind::Response,
                &mut output,
                &mut checksum,
            ),
            Err(FrameError::InvalidLength),
        );
        assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);

        assert_eq!(
            decode_frame(
                &wire_frame(header, &payload[..length]),
                FrameKind::Response,
                &mut checksum,
            ),
            Err(FrameError::InvalidLength),
        );
    }
}

#[test]
fn response_payload_mismatch_preserves_output() {
    for (opcode, length, payload) in [
        (Opcode::Ack, 0, &[1][..]),
        (Opcode::Ack, 2, &[1][..]),
        (Opcode::Nack, 1, &[][..]),
        (Opcode::Nack, 1, &[1, 2][..]),
    ] {
        let mut output = [0xA5; MAX_FRAME_SIZE];

        assert_eq!(
            encode_frame(
                header(opcode, 3, length),
                payload,
                FrameKind::Response,
                &mut output,
                &mut checksum,
            ),
            Err(FrameError::InvalidLength),
        );
        assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);
    }
}

#[test]
fn oversized_ack_is_rejected_without_modifying_output() {
    let header = header(
        Opcode::Ack,
        0x20,
        (MAX_PAYLOAD_SIZE + 1) as u16,
    );
    let mut output = [0xA5; MAX_FRAME_SIZE];

    assert_eq!(
        encode_frame(
            header,
            &[],
            FrameKind::Response,
            &mut output,
            &mut checksum,
        ),
        Err(FrameError::PayloadTooLarge),
    );
    assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);

    assert_eq!(
        decode_frame(
            &wire_frame(header, &[]),
            FrameKind::Response,
            &mut checksum,
        ),
        Err(FrameError::PayloadTooLarge),
    );
}

#[test]
fn response_header_payload_and_crc_corruption_is_detected() {
    for (opcode, value) in [(Opcode::Ack, 3), (Opcode::Nack, 5)] {
        let original = wire_frame(header(opcode, 1, 1), &[value]);

        for position in [1, HEADER_SIZE, original.len() - 1] {
            let mut damaged = original.clone();
            damaged[position] ^= 1;

            assert_eq!(
                decode_frame(
                    &damaged,
                    FrameKind::Response,
                    &mut checksum,
                ),
                Err(FrameError::CrcMismatch),
            );
        }
    }
}

#[test]
fn response_truncation_and_trailing_bytes_are_rejected() {
    for opcode in [Opcode::Ack, Opcode::Nack] {
        let original = wire_frame(header(opcode, 1, 1), &[5]);

        for length in 0..original.len() {
            assert_eq!(
                decode_frame(
                    &original[..length],
                    FrameKind::Response,
                    &mut checksum,
                ),
                Err(FrameError::InvalidLength),
            );
        }

        let mut extra = original;
        extra.push(0);

        assert_eq!(
            decode_frame(
                &extra,
                FrameKind::Response,
                &mut checksum,
            ),
            Err(FrameError::InvalidLength),
        );
    }
}

#[test]
fn insufficient_response_output_space_preserves_buffer() {
    for (opcode, payload) in [
        (Opcode::Ack, &[][..]),
        (Opcode::Ack, &[3][..]),
        (Opcode::Nack, &[5][..]),
    ] {
        let required = HEADER_SIZE + payload.len() + CRC_SIZE;
        let mut output = [0xA5; MAX_FRAME_SIZE];

        assert_eq!(
            encode_frame(
                header(opcode, 1, payload.len() as u16),
                payload,
                FrameKind::Response,
                &mut output[..required - 1],
                &mut checksum,
            ),
            Err(FrameError::BufferTooSmall),
        );
        assert_eq!(output, [0xA5; MAX_FRAME_SIZE]);
    }
}
