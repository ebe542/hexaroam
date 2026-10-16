use core::cell::Cell;

use super::*;
use crate::{Calibration, PulseOutput, Servo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Busy,
}

#[derive(Default)]
struct OutputRecord {
    calls: Cell<usize>,
    last_pulse_us: Cell<Option<u32>>,
    error: Cell<Option<TestError>>,
}

struct TestOutput<'a>(&'a OutputRecord);

impl PulseOutput for TestOutput<'_> {
    type Error = TestError;

    fn set_pulse_width_us(
        &mut self,
        pulse_width_us: u32,
    ) -> Result<(), Self::Error> {
        self.0.calls.set(self.0.calls.get() + 1);

        if let Some(error) = self.0.error.get() {
            return Err(error);
        }

        self.0
            .last_pulse_us
            .set(Some(pulse_width_us));
        Ok(())
    }
}

fn calibration(lower_pulse: u32, upper_pulse: u32) -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        lower_pulse,
        Angle::from_centidegrees(4_500),
        upper_pulse,
    )
    .expect("Valid test calibration")
}

fn target(id: usize, centidegrees: i32) -> ServoTarget {
    ServoTarget {
        id: ServoId(id),
        angle: Angle::from_centidegrees(centidegrees),
    }
}

fn make_servos(records: &[OutputRecord; 2]) -> [Servo<TestOutput<'_>>; 2] {
    [
        Servo::new(
            calibration(1_000, 2_000),
            TestOutput(&records[0]),
        ),
        Servo::new(
            calibration(1_200, 1_800),
            TestOutput(&records[1]),
        ),
    ]
}

fn assert_untouched(records: &[OutputRecord; 2]) {
    for record in records {
        assert_eq!(record.calls.get(), 0);
        assert_eq!(record.last_pulse_us.get(), None);
    }
}

#[test]
fn construction_does_not_submit_commands() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    let [mut first, mut second] = make_servos(&records);

    let _bank = ServoBank::new([&mut first, &mut second]);

    assert_untouched(&records);
}

#[test]
fn empty_targets_succeed_without_output() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    assert_eq!(bank.set_targets(&[]), Ok(()));
    assert_untouched(&records);
}

#[test]
fn single_target_updates_only_the_selected_servo() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    assert_eq!(
        bank.set_targets(&[target(1, 4_500)]),
        Ok(())
    );

    assert_eq!(records[0].calls.get(), 0);
    assert_eq!(records[0].last_pulse_us.get(), None);
    assert_eq!(records[1].calls.get(), 1);
    assert_eq!(
        records[1].last_pulse_us.get(),
        Some(1_800)
    );
}

#[test]
fn different_angles_are_assigned_by_id() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    // Deliberately supply targets in reverse channel order.
    assert_eq!(
        bank.set_targets(&[target(1, 4_500), target(0, -4_500)]),
        Ok(()),
    );

    assert_eq!(records[0].calls.get(), 1);
    assert_eq!(
        records[0].last_pulse_us.get(),
        Some(1_000)
    );
    assert_eq!(records[1].calls.get(), 1);
    assert_eq!(
        records[1].last_pulse_us.get(),
        Some(1_800)
    );
}

#[test]
fn same_angle_uses_each_servos_own_calibration() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    assert_eq!(
        bank.set_targets(&[target(0, 2_250), target(1, 2_250)]),
        Ok(()),
    );

    assert_eq!(records[0].calls.get(), 1);
    assert_eq!(
        records[0].last_pulse_us.get(),
        Some(1_750)
    );
    assert_eq!(records[1].calls.get(), 1);
    assert_eq!(
        records[1].last_pulse_us.get(),
        Some(1_650)
    );
}

#[test]
fn unknown_id_rejects_all_targets_before_output() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    assert_eq!(
        bank.set_targets(&[target(0, 0), target(2, 0)]),
        Err(ServoBankError::UnknownServo(ServoId(2))),
    );

    assert_untouched(&records);
}

#[test]
fn duplicate_id_rejects_all_targets_before_output() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    assert_eq!(
        bank.set_targets(&[target(0, 0), target(0, 2_250)]),
        Err(ServoBankError::DuplicateServo(ServoId(
            0
        ))),
    );

    assert_untouched(&records);
}

#[test]
fn invalid_angle_rejects_all_targets_before_output() {
    for invalid_angle in [-4_501, 4_501] {
        let records = [OutputRecord::default(), OutputRecord::default()];
        let [mut first, mut second] = make_servos(&records);
        let mut bank = ServoBank::new([&mut first, &mut second]);

        assert_eq!(
            bank.set_targets(&[target(0, 0), target(1, invalid_angle),]),
            Err(ServoBankError::Calibration {
                id: ServoId(1),
                error: CalibrationError::AngleOutOfRange,
            }),
        );

        assert_untouched(&records);
    }
}

#[test]
fn first_output_error_stops_submission_without_retry() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    records[0]
        .error
        .set(Some(TestError::Busy));

    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    assert_eq!(
        bank.set_targets(&[target(0, 0), target(1, 0)]),
        Err(ServoBankError::Output {
            id: ServoId(0),
            error: ServoError::Output(TestError::Busy),
        }),
    );

    assert_eq!(records[0].calls.get(), 1);
    assert_eq!(records[0].last_pulse_us.get(), None);
    assert_eq!(records[1].calls.get(), 0);
    assert_eq!(records[1].last_pulse_us.get(), None);
}

#[test]
fn later_output_error_preserves_already_accepted_command() {
    let records = [OutputRecord::default(), OutputRecord::default()];
    records[1]
        .error
        .set(Some(TestError::Busy));

    let [mut first, mut second] = make_servos(&records);
    let mut bank = ServoBank::new([&mut first, &mut second]);

    assert_eq!(
        bank.set_targets(&[target(0, -4_500), target(1, 0)]),
        Err(ServoBankError::Output {
            id: ServoId(1),
            error: ServoError::Output(TestError::Busy),
        }),
    );

    assert_eq!(records[0].calls.get(), 1);
    assert_eq!(
        records[0].last_pulse_us.get(),
        Some(1_000)
    );
    assert_eq!(records[1].calls.get(), 1);
    assert_eq!(records[1].last_pulse_us.get(), None);
}
