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

use hexaroam_servo_signal::{PulseLimits, PulseTarget};

fn test_plan() -> SignalPlan<4> {
    SignalPlan::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: 1_000,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: 1_500,
            },
            PulseTarget {
                gpio: 2,
                pulse_width_us: 2_000,
            },
        ],
        &PulseLimits::new(1_000, 2_000).expect("Valid limits"),
        20_000,
    )
    .expect("Valid plan")
}

#[test]
fn complete_plan_preserves_mask_and_counter_order() {
    let encoded = EncodedSignalPlan::<8>::new(&test_plan(), 0b111)
        .expect("Valid encoding");

    assert_eq!(
        encoded.words(),
        &[
            0x8000_0007,
            24_989,
            0x8000_0006,
            12_489,
            0x8000_0004,
            12_489,
            0x8000_0000,
            449_989,
        ],
    );
}

#[test]
fn unused_buffer_capacity_is_not_exposed() {
    let encoded = EncodedSignalPlan::<12>::new(&test_plan(), 0b111)
        .expect("Valid encoding");

    assert_eq!(encoded.words().len(), 8);
    assert_eq!(
        encoded.words(),
        EncodedSignalPlan::<8>::new(&test_plan(), 0b111)
            .expect("Valid encoding")
            .words(),
    );
}

#[test]
fn insufficient_word_capacity_is_rejected() {
    assert_eq!(
        EncodedSignalPlan::<7>::new(&test_plan(), 0b111),
        Err(EncodingError::InsufficientCapacity),
    );

    assert_eq!(
        EncodedSignalPlan::<0>::new(&test_plan(), 0b111),
        Err(EncodingError::InsufficientCapacity),
    );
}

#[test]
fn complete_plan_rejects_invalid_configuration_masks() {
    for allowed_mask in [0, 1_u32 << 30, 1_u32 << 31] {
        assert_eq!(
            EncodedSignalPlan::<8>::new(&test_plan(), allowed_mask,),
            Err(EncodingError::InvalidGpioMask),
        );
    }
}

#[test]
fn complete_plan_rejects_unconfigured_outputs() {
    assert_eq!(
        EncodedSignalPlan::<8>::new(&test_plan(), 0b011),
        Err(EncodingError::UnconfiguredGpio),
    );
}

#[test]
fn overflow_in_final_entry_rejects_the_complete_plan() {
    let plan = SignalPlan::<2>::new(
        &[PulseTarget {
            gpio: 0,
            pulse_width_us: 1_000,
        }],
        &PulseLimits::new(1_000, 2_000).expect("Valid limits"),
        u32::MAX,
    )
    .expect("Valid logical plan");

    assert_eq!(
        EncodedSignalPlan::<4>::new(&plan, 1),
        Err(EncodingError::DurationOverflow),
    );
}

#[test]
fn capacity_depends_on_entries_not_number_of_servos() {
    let plan = SignalPlan::<4>::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: 1_500,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: 1_500,
            },
        ],
        &PulseLimits::new(1_000, 2_000).expect("Valid limits"),
        20_000,
    )
    .expect("Valid plan");

    let encoded = EncodedSignalPlan::<4>::new(&plan, 0b11)
        .expect("Two entries require four words");

    assert_eq!(
        encoded.words(),
        &[0x8000_0003, 37_489, 0x8000_0000, 462_489,],
    );
}
