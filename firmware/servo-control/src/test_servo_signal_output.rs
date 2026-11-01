extern crate std;

use self::std::vec::Vec;
use super::*;
use crate::{
    Angle, Calibration, ServoBank, ServoBankError, ServoId, ServoTarget,
};
use hexaroam_servo_signal::{
    SignalOutputState, SignalPlan, SignalPlanEntry, SignalPlanLifecycle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Busy,
    Faulted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestLifecycleError {
    AlreadyActive,
    Faulted,
    StopFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedPlan {
    period_us: u32,
    entries: Vec<SignalPlanEntry>,
}

#[derive(Default)]
struct TestOutput {
    calls: usize,
    service_calls: usize,
    accepted: Option<RecordedPlan>,
    error: Option<TestError>,
    start_calls: usize,
    stop_calls: usize,
    active: bool,
    start_error: Option<TestLifecycleError>,
    stop_error: bool,
    active_after_error: bool,
}

impl SignalPlanOutput for TestOutput {
    type Error = TestError;

    fn submit_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::Error> {
        self.calls += 1;

        if let Some(error) = self.error {
            return Err(error);
        }

        self.accepted = Some(RecordedPlan {
            period_us: plan.period_us(),
            entries: plan.entries().to_vec(),
        });

        Ok(())
    }

    fn service(&mut self) {
        self.service_calls += 1;
    }
}

impl SignalPlanLifecycle for TestOutput {
    type LifecycleError = TestLifecycleError;

    fn start_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::LifecycleError> {
        self.start_calls += 1;

        if self.active {
            return Err(TestLifecycleError::AlreadyActive);
        }

        if let Some(error) = self.start_error {
            self.active = self.active_after_error;
            return Err(error);
        }

        self.accepted = Some(RecordedPlan {
            period_us: plan.period_us(),
            entries: plan.entries().to_vec(),
        });
        self.active = true;

        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::LifecycleError> {
        self.stop_calls += 1;

        if self.stop_error {
            self.active = self.active_after_error;
            return Err(TestLifecycleError::StopFailed);
        }

        self.active = false;

        Ok(())
    }

    fn state(&self) -> SignalOutputState {
        if self.active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        }
    }
}

fn limits() -> PulseLimits {
    PulseLimits::new(1_000, 2_000).expect("Valid limits")
}

fn adapter() -> ServoSignalOutput<TestOutput, 2, 3> {
    ServoSignalOutput::new(
        TestOutput::default(),
        [7, 2],
        limits(),
        20_000,
    )
    .expect("Valid configuration")
}

fn entry(gpio_mask: u32, duration_us: u32) -> SignalPlanEntry {
    SignalPlanEntry {
        gpio_mask,
        duration_us,
    }
}

#[test]
fn construction_does_not_submit_or_service_output() {
    let adapter = adapter();

    assert_eq!(adapter.output.calls, 0);
    assert_eq!(adapter.output.service_calls, 0);
    assert_eq!(adapter.output.accepted, None);
}

#[test]
fn empty_output_configuration_is_rejected() {
    let result = ServoSignalOutput::<_, 0, 1>::new(
        TestOutput::default(),
        [],
        limits(),
        20_000,
    );

    assert_eq!(
        result.err(),
        Some(ServoSignalOutputInitError::EmptyOutputs),
    );
}

#[test]
fn gpio_outside_generic_signal_range_is_rejected() {
    let result = ServoSignalOutput::<_, 2, 3>::new(
        TestOutput::default(),
        [0, 32],
        limits(),
        20_000,
    );

    assert_eq!(
        result.err(),
        Some(ServoSignalOutputInitError::InvalidGpio(
            32
        )),
    );
}

#[test]
fn duplicate_gpio_is_rejected() {
    let result = ServoSignalOutput::<_, 2, 3>::new(
        TestOutput::default(),
        [7, 7],
        limits(),
        20_000,
    );

    assert_eq!(
        result.err(),
        Some(ServoSignalOutputInitError::DuplicateGpio(7)),
    );
}

#[test]
fn zero_period_is_rejected() {
    let result = ServoSignalOutput::<_, 2, 3>::new(
        TestOutput::default(),
        [7, 2],
        limits(),
        0,
    );

    assert_eq!(
        result.err(),
        Some(ServoSignalOutputInitError::InvalidPeriod),
    );
}

#[test]
fn capacity_must_accommodate_distinct_pulse_end_times() {
    let result = ServoSignalOutput::<_, 2, 2>::new(
        TestOutput::default(),
        [7, 2],
        limits(),
        20_000,
    );

    assert_eq!(
        result.err(),
        Some(ServoSignalOutputInitError::InsufficientCapacity),
    );
}

#[test]
fn wrong_pulse_count_does_not_submit_a_plan() {
    let mut adapter = adapter();

    let batches: &[&[u32]] = &[&[], &[1_000], &[1_000, 1_500, 2_000]];

    for pulses in batches {
        assert_eq!(
            adapter.submit_pulses(pulses),
            Err(
                ServoSignalOutputError::PulseCountMismatch {
                    expected: 2,
                    actual: pulses.len(),
                }
            ),
        );
    }

    assert_eq!(adapter.output.calls, 0);
    assert_eq!(adapter.output.accepted, None);
}

#[test]
fn servo_id_order_maps_to_configured_gpios() {
    let mut adapter = adapter();

    assert_eq!(
        adapter.submit_pulses(&[1_000, 2_000]),
        Ok(()),
    );

    assert_eq!(adapter.output.calls, 1);

    let plan = adapter
        .output
        .accepted
        .as_ref()
        .unwrap();

    assert_eq!(plan.period_us, 20_000);
    assert_eq!(
        plan.entries.as_slice(),
        &[
            entry((1 << 7) | (1 << 2), 1_000),
            entry(1 << 2, 1_000),
            entry(0, 18_000),
        ],
    );
}

#[test]
fn equal_pulse_widths_share_one_end_event() {
    let mut adapter = adapter();

    assert_eq!(
        adapter.submit_pulses(&[1_500, 1_500]),
        Ok(()),
    );

    assert_eq!(adapter.output.calls, 1);

    let plan = adapter
        .output
        .accepted
        .as_ref()
        .unwrap();

    assert_eq!(
        plan.entries.as_slice(),
        &[entry((1 << 7) | (1 << 2), 1_500), entry(0, 18_500),],
    );
}

#[test]
fn invalid_pulse_after_a_valid_pulse_prevents_submission() {
    let mut adapter = adapter();

    for pulse in [0, 999, 2_001] {
        assert_eq!(
            adapter.submit_pulses(&[1_500, pulse]),
            Err(ServoSignalOutputError::SignalPlan(
                SignalPlanError::InvalidPulseWidth(2),
            )),
        );
    }

    assert_eq!(adapter.output.calls, 0);
    assert_eq!(adapter.output.accepted, None);
}

#[test]
fn pulse_equal_to_period_is_rejected_even_if_limits_allow_it() {
    let mut adapter = ServoSignalOutput::<_, 2, 3>::new(
        TestOutput::default(),
        [7, 2],
        PulseLimits::new(1, 20_000).unwrap(),
        20_000,
    )
    .expect("Nonzero period and valid mapping");

    assert_eq!(
        adapter.submit_pulses(&[1_000, 20_000]),
        Err(ServoSignalOutputError::SignalPlan(
            SignalPlanError::InvalidPulseWidth(2),
        )),
    );

    assert_eq!(adapter.output.calls, 0);
}

#[test]
fn backend_errors_are_forwarded_without_retry_or_replacement() {
    for error in [TestError::Busy, TestError::Faulted] {
        let mut adapter = adapter();

        adapter
            .submit_pulses(&[1_500, 1_500])
            .expect("Initial plan must succeed");

        let previous = adapter.output.accepted.clone();
        adapter.output.error = Some(error);

        assert_eq!(
            adapter.submit_pulses(&[1_000, 2_000]),
            Err(ServoSignalOutputError::Output(error)),
        );

        assert_eq!(adapter.output.calls, 2);
        assert_eq!(adapter.output.accepted, previous);
    }
}

#[test]
fn service_reaches_backend_without_submitting_targets() {
    let mut adapter = adapter();

    adapter.service();
    adapter.service();

    assert_eq!(adapter.output.service_calls, 2);
    assert_eq!(adapter.output.calls, 0);
    assert_eq!(adapter.output.accepted, None);
}

#[test]
fn bank_does_not_commit_angles_when_plan_validation_fails() {
    let adapter = ServoSignalOutput::<_, 2, 3>::new(
        TestOutput::default(),
        [7, 2],
        PulseLimits::new(1_200, 1_800).unwrap(),
        20_000,
    )
    .expect("Valid configuration");

    let calibration = Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .unwrap();

    let mut bank = ServoBank::new([calibration, calibration], adapter);

    assert_eq!(
        bank.set_targets(&[
            ServoTarget {
                id: ServoId(0),
                angle: Angle::from_centidegrees(-4_500),
            },
            ServoTarget {
                id: ServoId(1),
                angle: Angle::from_centidegrees(0),
            },
        ]),
        Err(ServoBankError::Output(
            ServoSignalOutputError::SignalPlan(
                SignalPlanError::InvalidPulseWidth(7),
            ),
        )),
    );

    assert_eq!(
        bank.commanded_angle(ServoId(0)),
        Ok(None)
    );
    assert_eq!(
        bank.commanded_angle(ServoId(1)),
        Ok(None)
    );
}

#[test]
fn lifecycle_construction_preserves_backend_state_without_actions() {
    for active in [false, true] {
        let output = TestOutput {
            active,
            ..TestOutput::default()
        };

        let adapter =
            ServoSignalOutput::<_, 2, 3>::new(output, [7, 2], limits(), 20_000)
                .unwrap();

        let expected = if active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        };

        assert_eq!(adapter.state(), expected);
        assert_eq!(adapter.output.start_calls, 0);
        assert_eq!(adapter.output.stop_calls, 0);
        assert_eq!(adapter.output.calls, 0);
        assert_eq!(adapter.output.service_calls, 0);
        assert_eq!(adapter.output.accepted, None);
    }
}

#[test]
fn lifecycle_start_builds_initial_plan_in_servo_id_order() {
    let mut adapter = adapter();

    assert_eq!(
        adapter.start_pulses(&[1_000, 2_000]),
        Ok(())
    );

    assert_eq!(
        adapter.state(),
        SignalOutputState::Active
    );
    assert_eq!(adapter.output.start_calls, 1);
    assert_eq!(adapter.output.calls, 0);
    assert_eq!(adapter.output.stop_calls, 0);

    let plan = adapter
        .output
        .accepted
        .as_ref()
        .unwrap();

    assert_eq!(plan.period_us, 20_000);
    assert_eq!(
        plan.entries.as_slice(),
        &[
            entry((1 << 7) | (1 << 2), 1_000),
            entry(1 << 2, 1_000),
            entry(0, 18_000),
        ],
    );
}

#[test]
fn lifecycle_start_rejects_wrong_pulse_counts_before_backend() {
    let mut adapter = adapter();
    let batches: &[&[u32]] = &[&[], &[1_000], &[1_000, 1_500, 2_000]];

    for pulses in batches {
        assert_eq!(
            adapter.start_pulses(pulses),
            Err(
                ServoSignalOutputError::PulseCountMismatch {
                    expected: 2,
                    actual: pulses.len(),
                }
            ),
        );
    }

    assert_eq!(adapter.output.start_calls, 0);
    assert_eq!(adapter.output.calls, 0);
    assert_eq!(adapter.output.accepted, None);
    assert_eq!(
        adapter.state(),
        SignalOutputState::Inactive
    );
}

#[test]
fn lifecycle_start_validates_all_pulses_before_backend() {
    let mut adapter = adapter();

    for invalid in [0, 999, 2_001] {
        assert_eq!(
            adapter.start_pulses(&[1_500, invalid]),
            Err(ServoSignalOutputError::SignalPlan(
                SignalPlanError::InvalidPulseWidth(2),
            )),
        );
    }

    assert_eq!(adapter.output.start_calls, 0);
    assert_eq!(adapter.output.calls, 0);
    assert_eq!(adapter.output.accepted, None);
    assert_eq!(
        adapter.state(),
        SignalOutputState::Inactive
    );

    // Limits alone do not make a pulse equal to the period valid.
    let mut adapter = ServoSignalOutput::<_, 2, 3>::new(
        TestOutput::default(),
        [7, 2],
        PulseLimits::new(1, 20_000).unwrap(),
        20_000,
    )
    .unwrap();

    assert_eq!(
        adapter.start_pulses(&[1_000, 20_000]),
        Err(ServoSignalOutputError::SignalPlan(
            SignalPlanError::InvalidPulseWidth(2),
        )),
    );
    assert_eq!(adapter.output.start_calls, 0);
    assert_eq!(adapter.output.accepted, None);
    assert_eq!(
        adapter.state(),
        SignalOutputState::Inactive
    );
}

#[test]
fn lifecycle_start_errors_are_forwarded_without_retry() {
    for active_after_error in [false, true] {
        let mut adapter = adapter();
        adapter.output.start_error = Some(TestLifecycleError::Faulted);
        adapter.output.active_after_error = active_after_error;

        assert_eq!(
            adapter.start_pulses(&[1_000, 2_000]),
            Err(ServoSignalOutputError::Output(
                TestLifecycleError::Faulted,
            )),
        );

        let expected = if active_after_error {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        };

        assert_eq!(adapter.state(), expected);
        assert_eq!(adapter.output.start_calls, 1);
        assert_eq!(adapter.output.stop_calls, 0);
        assert_eq!(adapter.output.calls, 0);
        assert_eq!(adapter.output.accepted, None);
    }
}

#[test]
fn lifecycle_repeated_start_preserves_previous_plan() {
    let mut adapter = adapter();

    assert_eq!(
        adapter.start_pulses(&[1_500, 1_500]),
        Ok(())
    );
    let previous = adapter.output.accepted.clone();

    assert_eq!(
        adapter.start_pulses(&[1_000, 2_000]),
        Err(ServoSignalOutputError::Output(
            TestLifecycleError::AlreadyActive,
        )),
    );

    assert_eq!(
        adapter.state(),
        SignalOutputState::Active
    );
    assert_eq!(adapter.output.start_calls, 2);
    assert_eq!(adapter.output.accepted, previous);
    assert_eq!(adapter.output.calls, 0);
}

#[test]
fn lifecycle_stop_and_restart_are_forwarded() {
    let mut adapter = adapter();

    assert_eq!(
        adapter.start_pulses(&[1_000, 2_000]),
        Ok(())
    );
    assert_eq!(adapter.stop(), Ok(()));
    assert_eq!(
        adapter.state(),
        SignalOutputState::Inactive
    );

    // Repeated stop stays inactive.
    assert_eq!(adapter.stop(), Ok(()));
    assert_eq!(
        adapter.state(),
        SignalOutputState::Inactive
    );
    assert_eq!(adapter.output.stop_calls, 2);

    assert_eq!(
        adapter.start_pulses(&[1_500, 1_500]),
        Ok(())
    );
    assert_eq!(
        adapter.state(),
        SignalOutputState::Active
    );
    assert_eq!(adapter.output.start_calls, 2);

    let plan = adapter
        .output
        .accepted
        .as_ref()
        .unwrap();

    assert_eq!(
        plan.entries.as_slice(),
        &[entry((1 << 7) | (1 << 2), 1_500), entry(0, 18_500),],
    );
    assert_eq!(adapter.output.calls, 0);
}

#[test]
fn lifecycle_stop_failure_preserves_backend_state_without_retry() {
    let mut adapter = adapter();

    assert_eq!(
        adapter.start_pulses(&[1_000, 2_000]),
        Ok(())
    );
    let previous = adapter.output.accepted.clone();

    adapter.output.stop_error = true;
    adapter.output.active_after_error = true;

    assert_eq!(
        adapter.stop(),
        Err(TestLifecycleError::StopFailed),
    );

    assert_eq!(
        adapter.state(),
        SignalOutputState::Active
    );
    assert_eq!(adapter.output.stop_calls, 1);
    assert_eq!(adapter.output.start_calls, 1);
    assert_eq!(adapter.output.accepted, previous);
    assert_eq!(adapter.output.calls, 0);

    adapter.output.stop_error = false;

    assert_eq!(adapter.stop(), Ok(()));
    assert_eq!(
        adapter.state(),
        SignalOutputState::Inactive
    );
    assert_eq!(adapter.output.stop_calls, 2);
}
