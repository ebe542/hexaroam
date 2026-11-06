use super::{
    CobsReceiveError, CobsReceiver, MAX_DECODED_SIZE, MAX_ENCODED_SIZE,
};

fn feed_without_frame(receiver: &mut CobsReceiver, bytes: &[u8]) {
    for &byte in bytes {
        assert_eq!(receiver.push(byte), Ok(None));
    }
}

#[test]
fn ignores_bytes_before_first_delimiter() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[2, 0x11, 3, 0x22, 0x33]);
    assert_eq!(receiver.push(0), Ok(None));
}

#[test]
fn ignores_consecutive_delimiters() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 0, 0, 0]);
}

#[test]
fn delivers_frame_only_at_end_delimiter() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 3, 0x11, 0x22]);

    assert_eq!(
        receiver.push(0),
        Ok(Some(&[0x11, 0x22][..]))
    );
}

#[test]
fn restores_zeros_in_payload() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 1, 2, 0x11, 1]);

    assert_eq!(
        receiver.push(0),
        Ok(Some(&[0, 0x11, 0][..]))
    );
}

#[test]
fn delivers_multiple_frames_with_shared_boundary() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 2, 0x11]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x11][..])));

    feed_without_frame(&mut receiver, &[2, 0x22]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x22][..])));
}

#[test]
fn accepts_separate_start_and_end_delimiters() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 2, 0x11]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x11][..])));

    feed_without_frame(&mut receiver, &[0, 0, 2, 0x22]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x22][..])));
}

#[test]
fn allows_empty_decoded_content_for_later_validation() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 1]);

    assert_eq!(receiver.push(0), Ok(Some(&[][..])));
}

#[test]
fn rejects_truncated_block_at_end_delimiter() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 3, 0x11]);

    assert_eq!(
        receiver.push(0),
        Err(CobsReceiveError::InvalidEncoding),
    );
}

#[test]
fn decode_error_keeps_boundary_for_next_frame() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 3, 0x11]);
    assert_eq!(
        receiver.push(0),
        Err(CobsReceiveError::InvalidEncoding),
    );

    feed_without_frame(&mut receiver, &[2, 0x22]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x22][..])));
}

#[test]
fn accepts_maximum_encoded_and_decoded_sizes() {
    let mut receiver = CobsReceiver::new();

    assert_eq!(MAX_ENCODED_SIZE, 256);
    assert_eq!(MAX_DECODED_SIZE, 254);

    feed_without_frame(&mut receiver, &[0, 255]);

    for _ in 0..254 {
        assert_eq!(receiver.push(0x55), Ok(None));
    }

    assert_eq!(receiver.push(1), Ok(None));

    let expected = [0x55; 254];
    assert_eq!(
        receiver.push(0),
        Ok(Some(&expected[..]))
    );
}

#[test]
fn rejects_encoded_overflow_before_end_delimiter() {
    let mut receiver = CobsReceiver::new();

    assert_eq!(receiver.push(0), Ok(None));

    for _ in 0..MAX_ENCODED_SIZE {
        assert_eq!(receiver.push(1), Ok(None));
    }

    assert_eq!(
        receiver.push(1),
        Err(CobsReceiveError::EncodedTooLong),
    );
}

#[test]
fn overflow_discards_remaining_bytes_until_next_delimiter() {
    let mut receiver = CobsReceiver::new();

    assert_eq!(receiver.push(0), Ok(None));

    for _ in 0..MAX_ENCODED_SIZE {
        assert_eq!(receiver.push(1), Ok(None));
    }

    assert_eq!(
        receiver.push(1),
        Err(CobsReceiveError::EncodedTooLong),
    );

    // This apparent frame suffix must not be treated as a fresh frame.
    feed_without_frame(&mut receiver, &[2, 0x11, 2, 0x22, 0]);

    feed_without_frame(&mut receiver, &[2, 0x33]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x33][..])));
}

#[test]
fn rejects_decoded_overflow_within_encoded_limit() {
    let mut receiver = CobsReceiver::new();

    assert_eq!(receiver.push(0), Ok(None));

    // 256 empty COBS blocks decode to 255 zeros.
    for _ in 0..MAX_ENCODED_SIZE {
        assert_eq!(receiver.push(1), Ok(None));
    }

    assert_eq!(
        receiver.push(0),
        Err(CobsReceiveError::DecodedTooLong),
    );

    feed_without_frame(&mut receiver, &[2, 0x44]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x44][..])));
}

#[test]
fn timeout_discards_partial_frame_and_requires_new_boundary() {
    let mut receiver = CobsReceiver::new();

    feed_without_frame(&mut receiver, &[0, 3, 0x11]);
    receiver.on_timeout();

    // Ignore the old suffix until its delimiter.
    feed_without_frame(&mut receiver, &[0x22, 0]);

    feed_without_frame(&mut receiver, &[2, 0x33]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x33][..])));
}

#[test]
fn repeated_timeout_is_harmless() {
    let mut receiver = CobsReceiver::new();

    receiver.on_timeout();
    receiver.on_timeout();

    feed_without_frame(&mut receiver, &[2, 0x11, 0, 2, 0x22]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x22][..])));
}

#[test]
fn missing_end_delimiter_is_replaced_by_next_start_delimiter() {
    let mut receiver = CobsReceiver::new();

    // The next frame's start delimiter closes this truncated frame.
    feed_without_frame(&mut receiver, &[0, 4, 0x11]);

    assert_eq!(
        receiver.push(0),
        Err(CobsReceiveError::InvalidEncoding),
    );

    feed_without_frame(&mut receiver, &[2, 0x55]);
    assert_eq!(receiver.push(0), Ok(Some(&[0x55][..])));
}
