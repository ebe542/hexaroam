use super::*;

fn limits() -> PulseLimits {
    PulseLimits::new(1_000, 2_000).expect("Valid test limits")
}

fn target(gpio: u8, pulse_width_us: u32) -> PulseTarget {
    PulseTarget {
        gpio,
        pulse_width_us,
    }
}

fn entry(gpio_mask: u32, duration_us: u32) -> SignalPlanEntry {
    SignalPlanEntry {
        gpio_mask,
        duration_us,
    }
}

#[test]
fn single_output_produces_high_and_low_entries() {
    let plan = SignalPlan::<2>::new(&[target(0, 1_500)], &limits(), 20_000)
        .expect("Valid plan");

    assert_eq!(
        plan.entries(),
        &[entry(1, 1_500), entry(0, 18_500)],
    );
    assert_eq!(plan.period_us(), 20_000);
}

#[test]
fn different_pulse_widths_are_sorted_by_end_time() {
    let plan = SignalPlan::<4>::new(
        &[target(2, 2_000), target(0, 1_000), target(1, 1_500)],
        &limits(),
        20_000,
    )
    .expect("Valid plan");

    assert_eq!(
        plan.entries(),
        &[
            entry(0b111, 1_000),
            entry(0b110, 500),
            entry(0b100, 500),
            entry(0, 18_000),
        ],
    );

    let total: u32 = plan
        .entries()
        .iter()
        .map(|entry| entry.duration_us)
        .sum();

    assert_eq!(total, plan.period_us());
}

#[test]
fn equal_end_times_are_combined() {
    let plan = SignalPlan::<2>::new(
        &[target(0, 1_500), target(1, 1_500), target(2, 1_500)],
        &limits(),
        20_000,
    )
    .expect("Two entries are sufficient");

    assert_eq!(
        plan.entries(),
        &[entry(0b111, 1_500), entry(0, 18_500)],
    );
}

#[test]
fn sparse_gpio_assignments_preserve_bit_positions() {
    let plan = SignalPlan::<3>::new(
        &[target(2, 1_000), target(7, 2_000)],
        &limits(),
        20_000,
    )
    .expect("Valid plan");

    assert_eq!(
        plan.entries(),
        &[
            entry((1 << 2) | (1 << 7), 1_000),
            entry(1 << 7, 1_000),
            entry(0, 18_000),
        ],
    );
}

#[test]
fn highest_mask_bit_is_supported_by_the_plan_format() {
    let plan = SignalPlan::<2>::new(&[target(31, 1_000)], &limits(), 20_000)
        .expect("Valid logical mask");

    assert_eq!(
        plan.entries(),
        &[entry(1_u32 << 31, 1_000), entry(0, 19_000)],
    );
}

#[test]
fn period_is_not_fixed_to_twenty_milliseconds() {
    let plan = SignalPlan::<2>::new(&[target(0, 1_000)], &limits(), 10_000)
        .expect("Valid plan");

    assert_eq!(
        plan.entries(),
        &[entry(1, 1_000), entry(0, 9_000)],
    );
    assert_eq!(plan.period_us(), 10_000);
}

#[test]
fn empty_targets_are_rejected() {
    assert_eq!(
        SignalPlan::<2>::new(&[], &limits(), 20_000),
        Err(SignalPlanError::EmptyTargets),
    );
}

#[test]
fn zero_period_is_rejected() {
    assert_eq!(
        SignalPlan::<2>::new(&[target(0, 1_000)], &limits(), 0,),
        Err(SignalPlanError::InvalidPeriod),
    );
}

#[test]
fn gpio_outside_the_mask_is_rejected() {
    for gpio in [32, u8::MAX] {
        assert_eq!(
            SignalPlan::<3>::new(
                &[target(0, 1_000), target(gpio, 1_500)],
                &limits(),
                20_000,
            ),
            Err(SignalPlanError::InvalidGpio(gpio)),
        );
    }
}

#[test]
fn duplicate_gpio_is_rejected() {
    assert_eq!(
        SignalPlan::<3>::new(
            &[target(2, 1_000), target(2, 1_500)],
            &limits(),
            20_000,
        ),
        Err(SignalPlanError::DuplicateGpio(2)),
    );
}

#[test]
fn pulse_width_outside_configured_limits_is_rejected() {
    for pulse_width_us in [999, 2_001, u32::MAX] {
        assert_eq!(
            SignalPlan::<2>::new(
                &[target(0, pulse_width_us)],
                &limits(),
                20_000,
            ),
            Err(SignalPlanError::InvalidPulseWidth(0)),
        );
    }
}

#[test]
fn zero_or_period_spanning_pulse_is_rejected() {
    let wide_limits = PulseLimits::new(0, 20_001).expect("Valid limits");

    for pulse_width_us in [0, 20_000, 20_001] {
        assert_eq!(
            SignalPlan::<2>::new(
                &[target(0, pulse_width_us)],
                &wide_limits,
                20_000,
            ),
            Err(SignalPlanError::InvalidPulseWidth(0)),
        );
    }
}

#[test]
fn insufficient_entry_capacity_is_rejected() {
    assert_eq!(
        SignalPlan::<2>::new(
            &[target(0, 1_000), target(1, 2_000)],
            &limits(),
            20_000,
        ),
        Err(SignalPlanError::InsufficientCapacity),
    );

    assert_eq!(
        SignalPlan::<0>::new(&[target(0, 1_000)], &limits(), 20_000,),
        Err(SignalPlanError::InsufficientCapacity),
    );
}
