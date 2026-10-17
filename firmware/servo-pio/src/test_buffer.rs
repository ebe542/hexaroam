use super::*;
use hexaroam_servo_signal::{PulseLimits, PulseTarget};

fn single_plan(gpio: u8, pulse_width_us: u32) -> SignalPlan<2> {
    SignalPlan::new(
        &[PulseTarget {
            gpio,
            pulse_width_us,
        }],
        &PulseLimits::new(1_000, 2_000).expect("Valid limits"),
        20_000,
    )
    .expect("Valid plan")
}

fn two_channel_plan() -> SignalPlan<3> {
    SignalPlan::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: 1_000,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: 2_000,
            },
        ],
        &PulseLimits::new(1_000, 2_000).expect("Valid limits"),
        20_000,
    )
    .expect("Valid plan")
}

#[test]
fn new_buffer_has_no_payload() {
    let buffer = OutputBuffer::<6>::new();

    assert!(buffer.words().is_empty());
}

#[test]
fn loading_exposes_the_complete_encoded_plan() {
    let mut buffer = OutputBuffer::<6>::new();

    assert_eq!(
        buffer.load(&single_plan(0, 1_000), 1),
        Ok(()),
    );

    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 24_989, 0x8000_0000, 474_989,],
    );
}

#[test]
fn replacing_with_a_shorter_plan_hides_previous_words() {
    let mut buffer = OutputBuffer::<6>::new();

    buffer
        .load(&two_channel_plan(), 0b11)
        .expect("Valid first plan");

    assert_eq!(
        buffer.words(),
        &[
            0x8000_0003,
            24_989,
            0x8000_0002,
            24_989,
            0x8000_0000,
            449_989,
        ],
    );

    buffer
        .load(&single_plan(0, 1_500), 0b11)
        .expect("Valid replacement");

    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 37_489, 0x8000_0000, 462_489,],
    );
}

#[test]
fn encoding_error_preserves_previous_payload() {
    let mut buffer = OutputBuffer::<6>::new();

    buffer
        .load(&single_plan(0, 1_000), 1)
        .expect("Valid first plan");

    assert_eq!(
        buffer.load(&single_plan(1, 1_500), 1),
        Err(EncodingError::UnconfiguredGpio),
    );

    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 24_989, 0x8000_0000, 474_989,],
    );
}

#[test]
fn insufficient_capacity_preserves_previous_payload() {
    let mut buffer = OutputBuffer::<4>::new();

    buffer
        .load(&single_plan(0, 1_000), 0b11)
        .expect("Valid first plan");

    assert_eq!(
        buffer.load(&two_channel_plan(), 0b11),
        Err(EncodingError::InsufficientCapacity),
    );

    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 24_989, 0x8000_0000, 474_989,],
    );
}
