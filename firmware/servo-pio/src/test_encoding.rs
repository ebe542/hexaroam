use super::*;

fn entry(gpio_mask: u32, duration_us: u32) -> SignalPlanEntry {
    SignalPlanEntry {
        gpio_mask,
        duration_us,
    }
}

#[test]
fn gpio_mask_is_marked_and_duration_is_encoded() {
    assert_eq!(
        encode_entry(entry(0b101, 1_000), 0b111),
        Ok([0x8000_0005, 24_989]),
    );
}

#[test]
fn all_low_is_encoded_as_a_nonzero_mask_word() {
    assert_eq!(
        encode_entry(entry(0, 18_000), 0b111),
        Ok([0x8000_0000, 449_989]),
    );
}

#[test]
fn one_microsecond_has_a_positive_counter() {
    assert_eq!(
        encode_entry(entry(1, 1), 1),
        Ok([0x8000_0001, 14]),
    );
}

#[test]
fn highest_rp2040_gpio_bit_is_preserved() {
    let gpio_bit = 1_u32 << 29;

    assert_eq!(
        encode_entry(entry(gpio_bit, 1_000), gpio_bit),
        Ok([0xA000_0000, 24_989]),
    );
}

#[test]
fn empty_or_unsupported_configuration_mask_is_rejected() {
    for allowed_mask in [0, 1_u32 << 30, 1_u32 << 31, u32::MAX] {
        assert_eq!(
            encode_entry(entry(0, 1_000), allowed_mask),
            Err(EncodingError::InvalidGpioMask),
        );
    }
}

#[test]
fn unconfigured_gpio_is_rejected() {
    for gpio_mask in [0b100, 1_u32 << 30, 1_u32 << 31] {
        assert_eq!(
            encode_entry(entry(gpio_mask, 1_000), 0b011),
            Err(EncodingError::UnconfiguredGpio),
        );
    }
}

#[test]
fn zero_duration_is_rejected() {
    assert_eq!(
        encode_entry(entry(1, 0), 1),
        Err(EncodingError::ZeroDuration),
    );
}

#[test]
fn largest_encodable_whole_microsecond_duration_is_accepted() {
    assert_eq!(
        encode_entry(entry(1, 171_798_692), 1),
        Ok([0x8000_0001, 4_294_967_289]),
    );
}

#[test]
fn excessive_duration_is_rejected() {
    for duration_us in [171_798_693, u32::MAX] {
        assert_eq!(
            encode_entry(entry(1, duration_us), 1),
            Err(EncodingError::DurationOverflow),
        );
    }
}
