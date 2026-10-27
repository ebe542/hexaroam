use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Busy,
    Faulted,
}

#[derive(Default)]
struct TestOutput {
    calls: usize,
    accepted: Option<[u32; 2]>,
    error: Option<TestError>,
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

    fn service(&mut self) {}
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
