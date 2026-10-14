use core::cell::Cell;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Busy,
}

struct TestOutput<'a> {
    last_pulse_us: &'a Cell<Option<u32>>,
    calls: &'a Cell<usize>,
    error: Option<TestError>,
}

impl PulseOutput for TestOutput<'_> {
    type Error = TestError;

    fn set_pulse_width_us(
        &mut self,
        pulse_width_us: u32,
    ) -> Result<(), Self::Error> {
        self.calls.set(self.calls.get() + 1);

        if let Some(error) = self.error {
            return Err(error);
        }

        self.last_pulse_us
            .set(Some(pulse_width_us));
        Ok(())
    }
}

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .expect("Valid calibration")
}

#[test]
fn construction_does_not_submit_a_command() {
    let last = Cell::new(None);
    let calls = Cell::new(0);

    let _servo = Servo::new(
        calibration(),
        TestOutput {
            last_pulse_us: &last,
            calls: &calls,
            error: None,
        },
    );

    assert_eq!(calls.get(), 0);
    assert_eq!(last.get(), None);
}

#[test]
fn valid_angle_submits_one_calibrated_pulse() {
    let last = Cell::new(None);
    let calls = Cell::new(0);

    let mut servo = Servo::new(
        calibration(),
        TestOutput {
            last_pulse_us: &last,
            calls: &calls,
            error: None,
        },
    );

    assert_eq!(
        servo.set_angle(Angle::from_centidegrees(2_250)),
        Ok(()),
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(last.get(), Some(1_750));
}

#[test]
fn invalid_angles_do_not_call_output() {
    let last = Cell::new(None);
    let calls = Cell::new(0);

    let mut servo = Servo::new(
        calibration(),
        TestOutput {
            last_pulse_us: &last,
            calls: &calls,
            error: None,
        },
    );

    for target in [-4_501, 4_501] {
        assert_eq!(
            servo.set_angle(Angle::from_centidegrees(target)),
            Err(ServoError::Calibration(
                CalibrationError::AngleOutOfRange,
            )),
        );
    }

    assert_eq!(calls.get(), 0);
    assert_eq!(last.get(), None);
}

#[test]
fn output_error_is_forwarded_without_retry() {
    let last = Cell::new(None);
    let calls = Cell::new(0);

    let mut servo = Servo::new(
        calibration(),
        TestOutput {
            last_pulse_us: &last,
            calls: &calls,
            error: Some(TestError::Busy),
        },
    );

    assert_eq!(
        servo.set_angle(Angle::from_centidegrees(0)),
        Err(ServoError::Output(TestError::Busy)),
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(last.get(), None);
}

#[test]
fn successive_commands_submit_their_own_pulse_widths() {
    let last = Cell::new(None);
    let calls = Cell::new(0);

    let mut servo = Servo::new(
        calibration(),
        TestOutput {
            last_pulse_us: &last,
            calls: &calls,
            error: None,
        },
    );

    for (index, (target, expected)) in
        [(-4_500, 1_000), (0, 1_500), (4_500, 2_000)]
            .into_iter()
            .enumerate()
    {
        assert_eq!(
            servo.set_angle(Angle::from_centidegrees(target)),
            Ok(()),
        );
        assert_eq!(calls.get(), index + 1);
        assert_eq!(last.get(), Some(expected));
    }
}
