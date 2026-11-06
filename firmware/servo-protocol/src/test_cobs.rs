use super::{CobsError, decode, encode};

fn assert_vector(input: &[u8], expected: &[u8]) {
    let mut encoded = [0xA5; 520];
    let length = encode(input, &mut encoded).unwrap();

    assert_eq!(length, expected.len());
    assert_eq!(&encoded[..length], expected);
    assert!(!encoded[..length].contains(&0));
    assert!(
        encoded[length..]
            .iter()
            .all(|&byte| byte == 0xA5)
    );

    let mut decoded = [0xA5; 520];
    let length = decode(expected, &mut decoded).unwrap();

    assert_eq!(length, input.len());
    assert_eq!(&decoded[..length], input);
    assert!(
        decoded[length..]
            .iter()
            .all(|&byte| byte == 0xA5)
    );
}

fn assert_round_trip(input: &[u8]) {
    let mut encoded = [0; 520];
    let encoded_length = encode(input, &mut encoded).unwrap();

    assert!(!encoded[..encoded_length].contains(&0));

    let mut decoded = [0; 520];
    let decoded_length =
        decode(&encoded[..encoded_length], &mut decoded).unwrap();

    assert_eq!(&decoded[..decoded_length], input);
}

#[test]
fn empty_input_encodes_as_one() {
    assert_vector(&[], &[1]);
}

#[test]
fn one_zero_encodes_as_two_empty_blocks() {
    assert_vector(&[0], &[1, 1]);
}

#[test]
fn consecutive_zeros_encode_as_empty_blocks() {
    assert_vector(&[0, 0, 0], &[1, 1, 1, 1]);
}

#[test]
fn nonzero_bytes_share_one_block() {
    assert_vector(
        &[0x11, 0x22, 0x33],
        &[4, 0x11, 0x22, 0x33],
    );
}

#[test]
fn leading_and_trailing_zeros_are_preserved() {
    assert_vector(&[0, 0x11, 0], &[1, 2, 0x11, 1]);
}

#[test]
fn embedded_zeros_split_blocks() {
    assert_vector(
        &[0x11, 0, 0x22, 0x33, 0, 0x44],
        &[2, 0x11, 3, 0x22, 0x33, 2, 0x44],
    );
}

#[test]
fn block_with_253_nonzero_bytes_uses_one_code() {
    let input = [0x55; 253];
    let mut expected = [0x55; 254];
    expected[0] = 254;

    assert_vector(&input, &expected);
}

#[test]
fn full_nonzero_block_has_empty_terminal_block() {
    let input = [0x55; 254];
    let mut expected = [0x55; 256];
    expected[0] = 255;
    expected[255] = 1;

    assert_vector(&input, &expected);
}

#[test]
fn byte_after_full_block_starts_next_block() {
    let input = [0x55; 255];
    let mut expected = [0x55; 257];
    expected[0] = 255;
    expected[255] = 2;

    assert_vector(&input, &expected);
}

#[test]
fn zero_after_full_block_is_preserved() {
    let mut input = [0x55; 255];
    input[254] = 0;

    let mut expected = [0x55; 257];
    expected[0] = 255;
    expected[255] = 1;
    expected[256] = 1;

    assert_vector(&input, &expected);
}

#[test]
fn decoder_accepts_full_block_without_terminal_empty_block() {
    let mut encoded = [0x55; 255];
    encoded[0] = 255;

    let mut output = [0; 254];

    assert_eq!(decode(&encoded, &mut output), Ok(254));
    assert_eq!(output, [0x55; 254]);
}

#[test]
fn every_byte_value_survives_round_trip() {
    let input: [u8; 256] = core::array::from_fn(|index| index as u8);

    assert_round_trip(&input);
}

#[test]
fn multiple_full_blocks_survive_round_trip() {
    assert_round_trip(&[0x55; 508]);
}

#[test]
fn exact_output_capacity_is_sufficient() {
    let mut encoded = [0; 3];
    assert_eq!(encode(&[0, 0x11], &mut encoded), Ok(3));
    assert_eq!(encoded, [1, 2, 0x11]);

    let mut decoded = [0; 2];
    assert_eq!(decode(&encoded, &mut decoded), Ok(2));
    assert_eq!(decoded, [0, 0x11]);

    assert_eq!(decode(&[1], &mut []), Ok(0));
}

#[test]
fn encoding_rejects_insufficient_output_capacity() {
    assert_eq!(
        encode(&[], &mut []),
        Err(CobsError::OutputTooSmall)
    );
    assert_eq!(
        encode(&[0x11], &mut [0; 1]),
        Err(CobsError::OutputTooSmall),
    );
    assert_eq!(
        encode(&[0x55; 254], &mut [0; 255]),
        Err(CobsError::OutputTooSmall),
    );
}

#[test]
fn decoding_rejects_insufficient_output_capacity() {
    assert_eq!(
        decode(&[2, 0x11], &mut []),
        Err(CobsError::OutputTooSmall),
    );
    assert_eq!(
        decode(&[1, 1], &mut []),
        Err(CobsError::OutputTooSmall),
    );
}

#[test]
fn decoding_rejects_empty_encoded_input() {
    assert_eq!(
        decode(&[], &mut [0; 8]),
        Err(CobsError::InvalidEncoding)
    );
}

#[test]
fn decoding_rejects_zero_anywhere_in_encoded_input() {
    for input in [&[0][..], &[1, 0][..], &[2, 0][..], &[3, 0x11, 0][..]] {
        assert_eq!(
            decode(input, &mut [0; 8]),
            Err(CobsError::InvalidEncoding),
        );
    }
}

#[test]
fn decoding_rejects_truncated_blocks() {
    for input in [&[2][..], &[3, 0x11][..], &[255, 0x11][..]] {
        assert_eq!(
            decode(input, &mut [0; 254]),
            Err(CobsError::InvalidEncoding),
        );
    }
}
