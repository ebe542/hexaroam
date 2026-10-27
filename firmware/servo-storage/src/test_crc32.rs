use super::crc32;

#[test]
fn empty_input_returns_zero() {
    assert_eq!(crc32(b""), 0);
}

#[test]
fn standard_check_vector_matches() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn text_vector_matches() {
    assert_eq!(
        crc32(b"The quick brown fox jumps over the lazy dog"),
        0x414F_A339,
    );
}

use super::{Checksum, SoftwareCrc32};

#[test]
fn software_adapter_matches_standard_check_vector() {
    let mut checksum = SoftwareCrc32;

    assert_eq!(
        checksum.checksum(b"123456789"),
        0xCBF4_3926
    );
}

#[test]
fn software_adapter_starts_each_calculation_independently() {
    let mut checksum = SoftwareCrc32;

    assert_eq!(
        checksum.checksum(b"123456789"),
        0xCBF4_3926
    );
    assert_eq!(checksum.checksum(b""), 0);
    assert_eq!(
        checksum.checksum(b"The quick brown fox jumps over the lazy dog"),
        0x414F_A339,
    );
    assert_eq!(
        checksum.checksum(b"123456789"),
        0xCBF4_3926
    );
}
