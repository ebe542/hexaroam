use super::*;
use hexaroam_servo_signal::{PulseLimits, PulseTarget};

fn single_plan(gpio: u8, pulse_width_us: u32, period_us: u32) -> SignalPlan<2> {
    SignalPlan::new(
        &[PulseTarget {
            gpio,
            pulse_width_us,
        }],
        &PulseLimits::new(1_000, 2_000).expect("Valid limits"),
        period_us,
    )
    .expect("Valid plan")
}

fn two_channel_plan(
    first_pulse_us: u32,
    second_pulse_us: u32,
) -> SignalPlan<3> {
    SignalPlan::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: first_pulse_us,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: second_pulse_us,
            },
        ],
        &PulseLimits::new(1_000, 2_000).expect("Valid limits"),
        20_000,
    )
    .expect("Valid plan")
}

#[test]
fn new_pending_plan_is_not_ready_and_does_not_fill_buffer() {
    let mut buffer = OutputBuffer::<6>::new();
    let pending: PendingSignalPlan<'_, 6> =
        PendingSignalPlan::new(&mut buffer, 20_000, 1);

    assert!(!pending.is_ready());

    let (buffer, ready) = pending.into_parts();

    assert!(!ready);
    assert!(buffer.words().is_empty());
}

#[test]
fn successful_submission_prepares_the_complete_payload() {
    let mut buffer = OutputBuffer::<4>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 1);

    assert_eq!(
        pending.submit(&single_plan(0, 1_500, 20_000)),
        Ok(()),
    );
    assert!(pending.is_ready());

    let (buffer, ready) = pending.into_parts();

    assert!(ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 37_489, 0x8000_0000, 462_489,],
    );
}

#[test]
fn another_submission_is_busy_and_preserves_ready_payload() {
    let mut buffer = OutputBuffer::<4>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 1);

    pending
        .submit(&single_plan(0, 1_000, 20_000))
        .expect("Valid first submission");

    assert_eq!(
        pending.submit(&single_plan(0, 2_000, 20_000)),
        Err(SubmissionError::Busy),
    );
    assert!(pending.is_ready());

    let (buffer, ready) = pending.into_parts();

    assert!(ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 24_989, 0x8000_0000, 474_989,],
    );
}

#[test]
fn different_period_preserves_previous_buffer_contents() {
    let mut buffer = OutputBuffer::<4>::new();
    buffer
        .load(&single_plan(0, 1_000, 20_000), 1)
        .expect("Valid previous payload");

    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 1);

    assert_eq!(
        pending.submit(&single_plan(0, 1_500, 10_000)),
        Err(SubmissionError::PeriodMismatch),
    );
    assert!(!pending.is_ready());

    let (buffer, ready) = pending.into_parts();

    assert!(!ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 24_989, 0x8000_0000, 474_989,],
    );
}

#[test]
fn missing_or_additional_outputs_are_rejected() {
    let mut buffer = OutputBuffer::<6>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 0b11);

    for gpio in [0, 2] {
        assert_eq!(
            pending.submit(&single_plan(gpio, 1_000, 20_000)),
            Err(SubmissionError::OutputMismatch),
        );
        assert!(!pending.is_ready());
    }

    let (buffer, ready) = pending.into_parts();

    assert!(!ready);
    assert!(buffer.words().is_empty());
}

#[test]
fn insufficient_capacity_preserves_previous_payload() {
    let mut buffer = OutputBuffer::<4>::new();
    buffer
        .load(&two_channel_plan(1_500, 1_500), 0b11)
        .expect("Two entries fit");

    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 0b11);

    assert_eq!(
        pending.submit(&two_channel_plan(1_000, 2_000)),
        Err(SubmissionError::Encoding(
            EncodingError::InsufficientCapacity,
        )),
    );
    assert!(!pending.is_ready());

    let (buffer, ready) = pending.into_parts();

    assert!(!ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0003, 37_489, 0x8000_0000, 462_489,],
    );
}

#[test]
fn all_configured_outputs_are_prepared_together() {
    let mut buffer = OutputBuffer::<6>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 0b11);

    assert_eq!(
        pending.submit(&two_channel_plan(1_000, 2_000)),
        Ok(()),
    );

    let (buffer, ready) = pending.into_parts();

    assert!(ready);
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
}

#[test]
fn restart_preparation_replaces_a_ready_plan() {
    let mut buffer = OutputBuffer::<4>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 1);

    pending
        .submit(&single_plan(0, 1_000, 20_000))
        .expect("Valid pending plan");

    assert_eq!(
        pending.prepare_restart(&single_plan(0, 2_000, 20_000)),
        Ok(()),
    );

    let (buffer, ready) = pending.into_parts();

    assert!(ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 49_989, 0x8000_0000, 449_989],
    );
}

#[test]
fn restart_period_mismatch_preserves_a_ready_plan() {
    let mut buffer = OutputBuffer::<4>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 1);

    pending
        .submit(&single_plan(0, 1_000, 20_000))
        .expect("Valid pending plan");

    assert_eq!(
        pending.prepare_restart(&single_plan(0, 1_500, 10_000)),
        Err(SubmissionError::PeriodMismatch),
    );

    let (buffer, ready) = pending.into_parts();

    assert!(ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 24_989, 0x8000_0000, 474_989],
    );
}

#[test]
fn restart_output_mismatch_preserves_a_ready_plan() {
    let mut buffer = OutputBuffer::<4>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 1);

    pending
        .submit(&single_plan(0, 1_000, 20_000))
        .expect("Valid pending plan");

    assert_eq!(
        pending.prepare_restart(&single_plan(1, 1_500, 20_000)),
        Err(SubmissionError::OutputMismatch),
    );

    let (buffer, ready) = pending.into_parts();

    assert!(ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0001, 24_989, 0x8000_0000, 474_989],
    );
}

#[test]
fn restart_capacity_error_preserves_a_ready_plan() {
    let mut buffer = OutputBuffer::<4>::new();
    let mut pending = PendingSignalPlan::new(&mut buffer, 20_000, 0b11);

    pending
        .submit(&two_channel_plan(1_500, 1_500))
        .expect("Equal pulse widths fit into four words");

    assert_eq!(
        pending.prepare_restart(&two_channel_plan(1_000, 2_000)),
        Err(SubmissionError::Encoding(
            EncodingError::InsufficientCapacity,
        )),
    );

    let (buffer, ready) = pending.into_parts();

    assert!(ready);
    assert_eq!(
        buffer.words(),
        &[0x8000_0003, 37_489, 0x8000_0000, 462_489],
    );
}
