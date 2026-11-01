use super::*;

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

#[derive(Default)]
struct TestOutput {
    calls: usize,
    accepted: Option<[u32; 2]>,
    error: Option<TestError>,
    start_calls: usize,
    stop_calls: usize,
    service_calls: usize,
    active: bool,
    start_error: Option<TestLifecycleError>,
    stop_error: bool,
    active_after_error: bool,
}

impl PulseBankOutput for TestOutput {
    type Error = TestError;

    fn submit_pulses(
        &mut self,
        pulse_widths_us: &[u32],
    ) -> Result<(), Self::Error> {
        self.calls += 1;

        let pulses: [u32; 2] = pulse_widths_us
            .try_into()
            .expect("Expected one complete two-servo batch");

        if let Some(error) = self.error {
            return Err(error);
        }

        self.accepted = Some(pulses);

        Ok(())
    }

    fn service(&mut self) {
        self.service_calls += 1;
    }
}

impl PulseBankLifecycle for TestOutput {
    type LifecycleError = TestLifecycleError;

    fn start_pulses(
        &mut self,
        pulse_widths_us: &[u32],
    ) -> Result<(), Self::LifecycleError> {
        self.start_calls += 1;

        if self.active {
            return Err(TestLifecycleError::AlreadyActive);
        }

        if let Some(error) = self.start_error {
            self.active = self.active_after_error;
            return Err(error);
        }

        self.accepted = Some(
            pulse_widths_us
                .try_into()
                .expect("Expected a complete two-servo batch"),
        );
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

fn angle(centidegrees: i32) -> Angle {
    Angle::from_centidegrees(centidegrees)
}

fn target(id: usize, centidegrees: i32) -> ServoTarget {
    ServoTarget {
        id: ServoId(id),
        angle: angle(centidegrees),
    }
}

fn calibration(lower_pulse: u32, upper_pulse: u32) -> Calibration {
    Calibration::new(
        angle(-4_500),
        lower_pulse,
        angle(4_500),
        upper_pulse,
    )
    .expect("Valid test calibration")
}

fn bank() -> ServoBank<TestOutput, 2> {
    ServoBank::new(
        [calibration(1_000, 2_000), calibration(1_200, 1_800)],
        TestOutput::default(),
    )
}

fn initialized_bank() -> ServoBank<TestOutput, 2> {
    let mut bank = bank();

    bank.set_targets(&[target(0, 0), target(1, 0)])
        .expect("Initial batch must succeed");

    bank
}

fn assert_angles(
    bank: &ServoBank<TestOutput, 2>,
    first: Option<i32>,
    second: Option<i32>,
) {
    assert_eq!(
        bank.commanded_angle(ServoId(0)),
        Ok(first.map(angle)),
    );
    assert_eq!(
        bank.commanded_angle(ServoId(1)),
        Ok(second.map(angle)),
    );
}

#[test]
fn construction_does_not_submit_or_claim_accepted_angles() {
    let bank = bank();

    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
    assert_angles(&bank, None, None);
}

#[test]
fn empty_initial_request_does_not_submit_or_initialize() {
    let mut bank = bank();

    assert_eq!(bank.set_targets(&[]), Ok(()));

    assert_eq!(bank.output.calls, 0);
    assert_angles(&bank, None, None);
}

#[test]
fn first_complete_request_submits_once_in_servo_id_order() {
    let mut bank = bank();

    assert_eq!(
        bank.set_targets(&[target(1, 4_500), target(0, -4_500),]),
        Ok(()),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_000, 1_800])
    );
    assert_angles(&bank, Some(-4_500), Some(4_500));
}

#[test]
fn incomplete_first_request_is_rejected_without_output() {
    let mut bank = bank();

    assert_eq!(
        bank.set_targets(&[target(0, 0)]),
        Err(ServoBankError::UninitializedServo(
            ServoId(1)
        )),
    );

    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
    assert_angles(&bank, None, None);
}

#[test]
fn partial_update_preserves_the_omitted_servo() {
    let mut bank = initialized_bank();

    assert_eq!(
        bank.set_targets(&[target(0, -4_500)]),
        Ok(()),
    );

    assert_eq!(bank.output.calls, 2);
    assert_eq!(
        bank.output.accepted,
        Some([1_000, 1_500])
    );
    assert_angles(&bank, Some(-4_500), Some(0));
}

#[test]
fn equal_angles_use_individual_calibrations() {
    let mut bank = bank();

    assert_eq!(
        bank.set_targets(&[target(0, 4_500), target(1, 4_500),]),
        Ok(()),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([2_000, 1_800])
    );
    assert_angles(&bank, Some(4_500), Some(4_500));
}

#[test]
fn unknown_servo_rejects_the_whole_request() {
    let mut bank = initialized_bank();

    assert_eq!(
        bank.set_targets(&[target(0, -4_500), target(2, 0),]),
        Err(ServoBankError::UnknownServo(ServoId(2))),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_500])
    );
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn duplicate_servo_rejects_the_whole_request() {
    let mut bank = initialized_bank();

    assert_eq!(
        bank.set_targets(&[target(0, -4_500), target(0, 4_500),]),
        Err(ServoBankError::DuplicateServo(ServoId(
            0
        ))),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_500])
    );
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn invalid_angle_after_a_valid_target_preserves_all_commands() {
    let mut bank = initialized_bank();

    assert_eq!(
        bank.set_targets(&[target(0, -4_500), target(1, 4_501),]),
        Err(ServoBankError::Calibration {
            id: ServoId(1),
            error: CalibrationError::AngleOutOfRange,
        }),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_500])
    );
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn busy_first_submission_does_not_initialize_commands() {
    let mut bank = bank();
    bank.output.error = Some(TestError::Busy);

    assert_eq!(
        bank.set_targets(&[target(0, 0), target(1, 0)]),
        Err(ServoBankError::Output(TestError::Busy)),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(bank.output.accepted, None);
    assert_angles(&bank, None, None);
}

#[test]
fn faulted_output_preserves_previous_accepted_commands() {
    let mut bank = initialized_bank();
    bank.output.error = Some(TestError::Faulted);

    assert_eq!(
        bank.set_targets(&[target(0, -4_500), target(1, 4_500),]),
        Err(ServoBankError::Output(
            TestError::Faulted
        )),
    );

    assert_eq!(bank.output.calls, 2);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_500])
    );
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn next_request_does_not_inherit_a_rejected_change() {
    let mut bank = initialized_bank();
    bank.output.error = Some(TestError::Busy);

    assert_eq!(
        bank.set_targets(&[target(0, -4_500)]),
        Err(ServoBankError::Output(TestError::Busy)),
    );
    assert_angles(&bank, Some(0), Some(0));

    bank.output.error = None;

    assert_eq!(
        bank.set_targets(&[target(1, 4_500)]),
        Ok(()),
    );

    assert_eq!(bank.output.calls, 3);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_800])
    );
    assert_angles(&bank, Some(0), Some(4_500));
}

#[test]
fn decreasing_pulse_calibration_is_supported() {
    let mut bank = ServoBank::new(
        [calibration(2_000, 1_000), calibration(1_200, 1_800)],
        TestOutput::default(),
    );

    assert_eq!(
        bank.set_targets(&[target(0, -4_500), target(1, -4_500),]),
        Ok(()),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([2_000, 1_200])
    );
    assert_angles(&bank, Some(-4_500), Some(-4_500));
}

#[test]
fn querying_an_unknown_servo_returns_an_error() {
    let bank = bank();

    assert_eq!(
        bank.commanded_angle(ServoId(2)),
        Err(ServoBankError::UnknownServo(ServoId(2))),
    );

    assert_eq!(bank.output.calls, 0);
}

#[test]
fn incomplete_initial_requests_do_not_accumulate_changes() {
    let mut bank = bank();

    assert_eq!(
        bank.set_targets(&[target(0, -4_500)]),
        Err(ServoBankError::UninitializedServo(
            ServoId(1)
        )),
    );

    assert_eq!(
        bank.set_targets(&[target(1, 4_500)]),
        Err(ServoBankError::UninitializedServo(
            ServoId(0)
        )),
    );

    assert_eq!(bank.output.calls, 0);
    assert_angles(&bank, None, None);
}

#[test]
fn empty_request_preserves_initialized_commands_without_output() {
    let mut bank = initialized_bank();

    assert_eq!(bank.set_targets(&[]), Ok(()));

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_500])
    );
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn preparation_allows_stepwise_initialization_without_output() {
    let mut bank = bank();

    assert_eq!(
        bank.prepare_targets(&[target(0, -4_500)]),
        Ok(()),
    );
    assert_angles(&bank, Some(-4_500), None);
    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);

    assert_eq!(
        bank.prepare_targets(&[target(1, 4_500)]),
        Ok(()),
    );
    assert_angles(&bank, Some(-4_500), Some(4_500));
    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
}

#[test]
fn preparation_accepts_complete_batches_and_partial_updates() {
    let mut bank = bank();

    assert_eq!(
        bank.prepare_targets(&[target(1, 4_500), target(0, -4_500),]),
        Ok(()),
    );

    assert_eq!(
        bank.prepare_targets(&[target(0, 0)]),
        Ok(()),
    );

    assert_angles(&bank, Some(0), Some(4_500));
    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
}

#[test]
fn invalid_preparation_preserves_all_previous_targets() {
    let cases = [
        (
            [target(0, -4_500), target(2, 0)],
            ServoBankError::UnknownServo(ServoId(2)),
        ),
        (
            [target(0, -4_500), target(0, 4_500)],
            ServoBankError::DuplicateServo(ServoId(0)),
        ),
        (
            [target(0, -4_500), target(1, 4_501)],
            ServoBankError::Calibration {
                id: ServoId(1),
                error: CalibrationError::AngleOutOfRange,
            },
        ),
    ];

    for (targets, expected) in cases {
        let mut bank = bank();

        assert_eq!(
            bank.prepare_targets(&[target(0, 0), target(1, 0)]),
            Ok(()),
        );
        assert_eq!(
            bank.prepare_targets(&targets),
            Err(expected),
        );

        assert_angles(&bank, Some(0), Some(0));
        assert_eq!(bank.output.calls, 0);
        assert_eq!(bank.output.accepted, None);
    }
}

#[test]
fn preparation_does_not_call_a_failing_output_backend() {
    let mut bank = bank();
    bank.output.error = Some(TestError::Faulted);

    assert_eq!(
        bank.prepare_targets(&[target(0, -4_500), target(1, 4_500),]),
        Ok(()),
    );

    assert_angles(&bank, Some(-4_500), Some(4_500));
    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
}

#[test]
fn empty_preparation_preserves_initial_and_prepared_state() {
    let mut bank = bank();

    assert_eq!(bank.prepare_targets(&[]), Ok(()));
    assert_angles(&bank, None, None);

    assert_eq!(
        bank.prepare_targets(&[target(0, 1_234)]),
        Ok(()),
    );
    assert_eq!(bank.prepare_targets(&[]), Ok(()));

    assert_angles(&bank, Some(1_234), None);
    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
}

#[test]
fn submission_requires_every_servo_to_be_prepared() {
    let mut bank = bank();

    assert_eq!(
        bank.submit_prepared(),
        Err(ServoBankError::UninitializedServo(
            ServoId(0)
        )),
    );
    assert_angles(&bank, None, None);
    assert_eq!(bank.output.calls, 0);

    assert_eq!(
        bank.prepare_targets(&[target(0, 0)]),
        Ok(()),
    );

    assert_eq!(
        bank.submit_prepared(),
        Err(ServoBankError::UninitializedServo(
            ServoId(1)
        )),
    );

    assert_angles(&bank, Some(0), None);
    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
}

#[test]
fn submission_uses_prepared_targets_and_individual_calibrations() {
    let mut bank = bank();

    assert_eq!(
        bank.prepare_targets(&[target(1, 4_500), target(0, -4_500),]),
        Ok(()),
    );

    assert_eq!(bank.submit_prepared(), Ok(()));

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_000, 1_800])
    );
    assert_angles(&bank, Some(-4_500), Some(4_500));
}

#[test]
fn failed_submission_preserves_prepared_targets_for_retry() {
    let mut bank = bank();

    assert_eq!(
        bank.prepare_targets(&[target(0, -4_500), target(1, 4_500),]),
        Ok(()),
    );

    bank.output.error = Some(TestError::Busy);

    assert_eq!(
        bank.submit_prepared(),
        Err(ServoBankError::Output(TestError::Busy)),
    );

    assert_angles(&bank, Some(-4_500), Some(4_500));
    assert_eq!(bank.output.calls, 1);
    assert_eq!(bank.output.accepted, None);

    bank.output.error = None;

    assert_eq!(bank.submit_prepared(), Ok(()));
    assert_eq!(bank.output.calls, 2);
    assert_eq!(
        bank.output.accepted,
        Some([1_000, 1_800])
    );
    assert_angles(&bank, Some(-4_500), Some(4_500));
}

#[test]
fn set_targets_can_submit_a_partial_update_after_preparation() {
    let mut bank = bank();

    assert_eq!(
        bank.prepare_targets(&[target(0, 0), target(1, 0)]),
        Ok(()),
    );

    assert_eq!(
        bank.set_targets(&[target(1, 4_500)]),
        Ok(()),
    );

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_800])
    );
    assert_angles(&bank, Some(0), Some(4_500));
}

#[test]
fn rejected_preparation_does_not_leak_into_later_submission() {
    let mut bank = bank();

    assert_eq!(
        bank.prepare_targets(&[target(0, 0), target(1, 0)]),
        Ok(()),
    );

    assert_eq!(
        bank.prepare_targets(&[target(0, -4_500), target(1, 4_501),]),
        Err(ServoBankError::Calibration {
            id: ServoId(1),
            error: CalibrationError::AngleOutOfRange,
        }),
    );

    assert_eq!(bank.submit_prepared(), Ok(()));

    assert_eq!(bank.output.calls, 1);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_500])
    );
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn lifecycle_state_comes_from_backend_without_starting_output() {
    for active in [false, true] {
        let output = TestOutput {
            active,
            ..TestOutput::default()
        };

        let bank = ServoBank::new(
            [calibration(1_000, 2_000), calibration(1_200, 1_800)],
            output,
        );

        let expected = if active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        };

        assert_eq!(bank.state(), expected);
        assert_angles(&bank, None, None);
        assert_eq!(bank.output.start_calls, 0);
        assert_eq!(bank.output.stop_calls, 0);
        assert_eq!(bank.output.calls, 0);
    }
}

#[test]
fn lifecycle_start_requires_all_targets_before_backend_call() {
    let mut bank = bank();

    assert_eq!(
        bank.start_prepared(),
        Err(ServoBankError::UninitializedServo(
            ServoId(0)
        )),
    );
    assert_eq!(bank.output.start_calls, 0);

    bank.prepare_targets(&[target(0, 0)])
        .unwrap();

    assert_eq!(
        bank.start_prepared(),
        Err(ServoBankError::UninitializedServo(
            ServoId(1)
        )),
    );

    assert_angles(&bank, Some(0), None);
    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );
    assert_eq!(bank.output.start_calls, 0);
    assert_eq!(bank.output.calls, 0);
    assert_eq!(bank.output.accepted, None);
}

#[test]
fn lifecycle_start_converts_all_targets_in_servo_id_order() {
    let mut bank = bank();

    bank.prepare_targets(&[target(1, 4_500), target(0, -4_500)])
        .unwrap();

    assert_eq!(bank.start_prepared(), Ok(()));

    assert_eq!(bank.state(), SignalOutputState::Active);
    assert_eq!(bank.output.start_calls, 1);
    assert_eq!(bank.output.calls, 0);
    assert_eq!(
        bank.output.accepted,
        Some([1_000, 1_800])
    );
    assert_angles(&bank, Some(-4_500), Some(4_500));
}

#[test]
fn lifecycle_start_error_preserves_targets_and_reports_backend_state() {
    for active_after_error in [false, true] {
        let mut bank = bank();

        bank.prepare_targets(&[target(0, -4_500), target(1, 4_500)])
            .unwrap();

        bank.output.start_error = Some(TestLifecycleError::Faulted);
        bank.output.active_after_error = active_after_error;

        assert_eq!(
            bank.start_prepared(),
            Err(ServoBankError::Output(
                TestLifecycleError::Faulted,
            )),
        );

        let expected = if active_after_error {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        };

        assert_eq!(bank.state(), expected);
        assert_angles(&bank, Some(-4_500), Some(4_500));
        assert_eq!(bank.output.start_calls, 1);
        assert_eq!(bank.output.stop_calls, 0);
        assert_eq!(bank.output.calls, 0);
        assert_eq!(bank.output.accepted, None);
    }
}

#[test]
fn lifecycle_repeated_start_is_rejected_without_changing_targets() {
    let mut bank = bank();
    bank.prepare_targets(&[target(0, 0), target(1, 0)])
        .unwrap();

    assert_eq!(bank.start_prepared(), Ok(()));
    let previous = bank.output.accepted;

    assert_eq!(
        bank.start_prepared(),
        Err(ServoBankError::Output(
            TestLifecycleError::AlreadyActive,
        )),
    );

    assert_eq!(bank.state(), SignalOutputState::Active);
    assert_eq!(bank.output.start_calls, 2);
    assert_eq!(bank.output.accepted, previous);
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn lifecycle_stop_preserves_targets_for_another_start() {
    let mut bank = bank();

    bank.prepare_targets(&[target(0, -4_500), target(1, 4_500)])
        .unwrap();
    bank.start_prepared().unwrap();

    assert_eq!(bank.stop(), Ok(()));
    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );
    assert_angles(&bank, Some(-4_500), Some(4_500));

    assert_eq!(bank.stop(), Ok(()));
    assert_eq!(bank.output.stop_calls, 2);

    // Change only one target while stopped.
    bank.prepare_targets(&[target(0, 0)])
        .unwrap();

    assert_eq!(bank.start_prepared(), Ok(()));
    assert_eq!(bank.state(), SignalOutputState::Active);
    assert_eq!(bank.output.start_calls, 2);
    assert_eq!(bank.output.calls, 0);
    assert_eq!(
        bank.output.accepted,
        Some([1_500, 1_800])
    );
    assert_angles(&bank, Some(0), Some(4_500));
}

#[test]
fn lifecycle_stop_error_preserves_targets_without_automatic_retry() {
    let mut bank = bank();
    bank.prepare_targets(&[target(0, 0), target(1, 0)])
        .unwrap();
    bank.start_prepared().unwrap();

    bank.output.stop_error = true;
    bank.output.active_after_error = true;

    assert_eq!(
        bank.stop(),
        Err(TestLifecycleError::StopFailed)
    );
    assert_eq!(bank.state(), SignalOutputState::Active);
    assert_eq!(bank.output.stop_calls, 1);
    assert_eq!(bank.output.start_calls, 1);
    assert_angles(&bank, Some(0), Some(0));

    bank.output.stop_error = false;

    assert_eq!(bank.stop(), Ok(()));
    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );
    assert_eq!(bank.output.stop_calls, 2);
    assert_angles(&bank, Some(0), Some(0));
}

#[test]
fn lifecycle_service_does_not_start_or_stop_output() {
    let mut bank = bank();
    bank.prepare_targets(&[target(0, 0), target(1, 0)])
        .unwrap();

    bank.service();

    assert_eq!(
        bank.state(),
        SignalOutputState::Inactive
    );
    assert_eq!(bank.output.service_calls, 1);
    assert_eq!(bank.output.start_calls, 0);
    assert_eq!(bank.output.stop_calls, 0);
    assert_eq!(bank.output.calls, 0);

    bank.start_prepared().unwrap();
    bank.service();

    assert_eq!(bank.state(), SignalOutputState::Active);
    assert_eq!(bank.output.service_calls, 2);
    assert_eq!(bank.output.start_calls, 1);
    assert_eq!(bank.output.stop_calls, 0);
    assert_angles(&bank, Some(0), Some(0));
}
