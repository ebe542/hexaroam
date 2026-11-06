use super::{
    CobsFrameError, CobsFrameReceiver, MAX_CONTENT_SIZE, MAX_WIRE_SIZE, encode,
};
use crate::{cobs, cobs_receiver::CobsReceiveError};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

fn crc32(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

// Builds wire data independently of the frame encoder under test.
// Input already contains any desired CRC or malformed decoded content.
fn raw_frame(decoded: &[u8], wire: &mut [u8; MAX_WIRE_SIZE]) -> usize {
    wire[0] = 0;
    let length = cobs::encode(decoded, &mut wire[1..]).unwrap();
    wire[length + 1] = 0;
    length + 2
}

fn feed_prefix(receiver: &mut CobsFrameReceiver, wire: &[u8]) {
    for &byte in wire {
        assert_eq!(
            receiver.push(byte, &mut crc32),
            Ok(None)
        );
    }
}

#[test]
fn encoder_adds_delimiters_and_known_little_endian_crc() {
    let mut wire = [0xA5; MAX_WIRE_SIZE];
    let length = encode(b"123456789", &mut wire, &mut crc32).unwrap();

    assert_eq!(wire[0], 0);
    assert_eq!(wire[length - 1], 0);
    assert!(!wire[1..length - 1].contains(&0));
    assert!(
        wire[length..]
            .iter()
            .all(|&byte| byte == 0xA5)
    );

    let mut decoded = [0; 254];
    let decoded_length =
        cobs::decode(&wire[1..length - 1], &mut decoded).unwrap();

    assert_eq!(decoded_length, 13);
    assert_eq!(&decoded[..9], b"123456789");
    assert_eq!(
        &decoded[9..13],
        &[0x26, 0x39, 0xF4, 0xCB]
    );
}

#[test]
fn content_with_zeros_round_trips() {
    let content = [3, 0, 0x11, 0, 0x22];
    let mut wire = [0; MAX_WIRE_SIZE];
    let length = encode(&content, &mut wire, &mut crc32).unwrap();
    let mut receiver = CobsFrameReceiver::new();

    feed_prefix(&mut receiver, &wire[..length - 1]);

    assert_eq!(
        receiver.push(wire[length - 1], &mut crc32),
        Ok(Some(&content[..])),
    );
}

#[test]
fn maximum_content_fits_wire_buffer() {
    assert_eq!(MAX_CONTENT_SIZE, 250);
    assert_eq!(MAX_WIRE_SIZE, 258);

    let content = [0x55; MAX_CONTENT_SIZE];
    let mut wire = [0; MAX_WIRE_SIZE];
    let length = encode(&content, &mut wire, &mut crc32).unwrap();
    let mut receiver = CobsFrameReceiver::new();

    assert!(length <= MAX_WIRE_SIZE);
    feed_prefix(&mut receiver, &wire[..length - 1]);

    assert_eq!(
        receiver.push(0, &mut crc32),
        Ok(Some(&content[..])),
    );
}

#[test]
fn encoder_rejects_empty_content_without_checksum_call() {
    let mut calls = 0;
    let result = encode(
        &[],
        &mut [0; MAX_WIRE_SIZE],
        &mut |_| {
            calls += 1;
            0
        },
    );

    assert_eq!(
        result,
        Err(CobsFrameError::EmptyContent)
    );
    assert_eq!(calls, 0);
}

#[test]
fn encoder_rejects_oversized_content_without_checksum_call() {
    let mut calls = 0;
    let result = encode(
        &[0; MAX_CONTENT_SIZE + 1],
        &mut [0; MAX_WIRE_SIZE],
        &mut |_| {
            calls += 1;
            0
        },
    );

    assert_eq!(
        result,
        Err(CobsFrameError::ContentTooLong)
    );
    assert_eq!(calls, 0);
}

#[test]
fn encoder_checks_exact_output_capacity() {
    // Content 1 and zero CRC encode as [2, 1, 1, 1, 1, 1].
    // Including both delimiters requires exactly eight bytes.
    let mut output = [0xA5; 8];
    assert_eq!(
        encode(&[1], &mut output, &mut |_| 0),
        Ok(8)
    );
    assert_eq!(output, [0, 2, 1, 1, 1, 1, 1, 0]);

    assert_eq!(
        encode(&[1], &mut [0; 7], &mut |_| 0),
        Err(CobsFrameError::OutputTooSmall),
    );
    assert_eq!(
        encode(&[1], &mut [], &mut |_| 0),
        Err(CobsFrameError::OutputTooSmall),
    );
}

#[test]
fn bad_crc_is_rejected_and_next_frame_is_received() {
    let mut decoded = [1, 0, 0, 0, 0];
    decoded[1..].copy_from_slice(&(crc32(&[1]) ^ 1).to_le_bytes());

    let mut wire = [0; MAX_WIRE_SIZE];
    let length = raw_frame(&decoded, &mut wire);
    let mut receiver = CobsFrameReceiver::new();

    feed_prefix(&mut receiver, &wire[..length - 1]);
    assert_eq!(
        receiver.push(0, &mut crc32),
        Err(CobsFrameError::CrcMismatch),
    );

    decoded[1..].copy_from_slice(&crc32(&[1]).to_le_bytes());
    let length = raw_frame(&decoded, &mut wire);

    // The preceding end delimiter already established a boundary.
    feed_prefix(&mut receiver, &wire[1..length - 1]);
    assert_eq!(
        receiver.push(0, &mut crc32),
        Ok(Some(&[1][..]))
    );
}

#[test]
fn short_decoded_frames_do_not_call_checksum() {
    let mut receiver = CobsFrameReceiver::new();
    let mut calls = 0;
    let mut checksum = |_: &[u8]| {
        calls += 1;
        0
    };

    for length in 0..5 {
        let mut wire = [0; MAX_WIRE_SIZE];
        let wire_length = raw_frame(&[0; 4][..length], &mut wire);

        for &byte in &wire[..wire_length - 1] {
            assert_eq!(
                receiver.push(byte, &mut checksum),
                Ok(None)
            );
        }

        assert_eq!(
            receiver.push(0, &mut checksum),
            Err(CobsFrameError::TooShort),
        );
    }

    assert_eq!(calls, 0);
}

#[test]
fn receiver_calls_supplied_checksum_once_on_complete_content() {
    let decoded = [0xFE, 0x78, 0x56, 0x34, 0x12];
    let mut wire = [0; MAX_WIRE_SIZE];
    let length = raw_frame(&decoded, &mut wire);
    let mut receiver = CobsFrameReceiver::new();

    // No checksum invocation while the frame is incomplete.
    for &byte in &wire[..length - 1] {
        assert_eq!(
            receiver.push(byte, &mut |_| panic!(
                "Premature checksum"
            )),
            Ok(None),
        );
    }

    let mut calls = 0;
    let result = receiver.push(0, &mut |content: &[u8]| {
        calls += 1;
        assert_eq!(content, &[0xFE]);
        0x1234_5678
    });

    // Unknown command bytes belong to the later command layer.
    assert_eq!(result, Ok(Some(&[0xFE][..])));
    assert_eq!(calls, 1);
}

#[test]
fn timeout_discards_partial_frame_and_recovers() {
    let mut receiver = CobsFrameReceiver::new();
    feed_prefix(&mut receiver, &[0, 6, 1, 2]);
    receiver.on_timeout();

    feed_prefix(&mut receiver, &[3, 4, 5, 0]);

    let mut wire = [0; MAX_WIRE_SIZE];
    let length = encode(&[1], &mut wire, &mut crc32).unwrap();

    feed_prefix(&mut receiver, &wire[..length - 1]);
    assert_eq!(
        receiver.push(0, &mut crc32),
        Ok(Some(&[1][..]))
    );
}

#[test]
fn malformed_cobs_does_not_call_checksum() {
    let mut receiver = CobsFrameReceiver::new();
    feed_prefix(&mut receiver, &[0, 3, 0x11]);

    assert_eq!(
        receiver.push(0, &mut |_| panic!(
            "Invalid COBS reached checksum"
        )),
        Err(CobsFrameError::Receive(
            CobsReceiveError::InvalidEncoding,
        )),
    );
}

#[test]
fn encoded_overflow_is_reported_without_checksum() {
    let mut receiver = CobsFrameReceiver::new();
    let mut checksum = |_: &[u8]| panic!("Overflow reached checksum");

    assert_eq!(
        receiver.push(0, &mut checksum),
        Ok(None)
    );

    for _ in 0..256 {
        assert_eq!(
            receiver.push(1, &mut checksum),
            Ok(None)
        );
    }

    assert_eq!(
        receiver.push(1, &mut checksum),
        Err(CobsFrameError::Receive(
            CobsReceiveError::EncodedTooLong,
        )),
    );

    assert_eq!(
        receiver.push(0, &mut checksum),
        Ok(None)
    );
}
