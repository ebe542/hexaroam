use super::*;
use crate::{
    OutputStartError, TargetReadError, TargetWriteError,
    test_support::FakeOutput,
};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_protocol::command::{Request, decode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Busy,
    Fault,
}

// Reuse existing validation and recording behavior. Only submission
// error classification is added for the new command-service tests.
struct TestOutput {
    inner: FakeOutput,
    busy: bool,
}

impl OutputBackend for TestOutput {
    type Error = TestError;

    fn is_busy_error(error: &Self::Error) -> bool {
        matches!(error, TestError::Busy)
    }

    fn start(&mut self) -> Result<(), OutputStartError<Self::Error>> {
        self.inner
            .start()
            .map_err(|error| match error {
                OutputStartError::InvalidTargets => {
                    OutputStartError::InvalidTargets
                }
                OutputStartError::Backend(()) => {
                    OutputStartError::Backend(TestError::Fault)
                }
            })
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        self.inner
            .stop()
            .map_err(|()| TestError::Fault)
    }

    fn state(&self) -> OutputState {
        self.inner.state()
    }

    fn target(&self, id: ServoId) -> Result<Angle, TargetReadError> {
        self.inner.target(id)
    }

    fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), TargetWriteError<Self::Error>> {
        if self.busy {
            return Err(TargetWriteError::Backend(
                TestError::Busy,
            ));
        }

        self.inner
            .set_targets(targets)
            .map_err(|error| match error {
                TargetWriteError::UnknownServo(id) => {
                    TargetWriteError::UnknownServo(id)
                }
                TargetWriteError::DuplicateServo(id) => {
                    TargetWriteError::DuplicateServo(id)
                }
                TargetWriteError::InvalidAngle(id) => {
                    TargetWriteError::InvalidAngle(id)
                }
                TargetWriteError::UninitializedServo(id) => {
                    TargetWriteError::UninitializedServo(id)
                }
                TargetWriteError::Backend(()) => {
                    TargetWriteError::Backend(TestError::Fault)
                }
            })
    }

    fn service(&mut self) {
        self.inner.service();
    }
}

type Service = CommandService<TestOutput, 2>;

fn service(calibration_valid: bool) -> Service {
    CommandService::new(
        TestOutput {
            inner: FakeOutput::inactive(),
            busy: false,
        },
        calibration_valid,
    )
}

fn write(
    service: &mut Service,
    first_servo: u8,
    angles: &[i32],
) -> Result<(), NackCode> {
    let mut content = [0; 250];
    content[0] = 0x03;
    content[1] = first_servo;

    for (index, angle) in angles.iter().enumerate() {
        let offset = 2 + index * 4;
        content[offset..offset + 4].copy_from_slice(&angle.to_le_bytes());
    }

    let length = 2 + angles.len() * 4;
    let Some(Request::SetTargets(batch)) = decode(&content[..length]).unwrap()
    else {
        panic!("Expected target request");
    };

    service.set_targets(batch)
}

fn running() -> Service {
    let mut service = service(true);
    service.set_enabled(true).unwrap();
    write(&mut service, 0, &[100, 200]).unwrap();
    service
}

fn accepted_angles(service: &Service) -> [i32; 2] {
    [
        service.output.inner.targets[0].as_centidegrees(),
        service.output.inner.targets[1].as_centidegrees(),
    ]
}

#[test]
fn missing_calibration_rejects_targets_without_changes() {
    let mut service = service(false);

    assert_eq!(
        write(&mut service, 0, &[100, 200]),
        Err(NackCode::InvalidState),
    );
    assert_eq!(accepted_angles(&service), [0, 0]);
    assert_eq!(service.output.inner.start_calls, 0);
    assert_eq!(service.status(), 0);
}

#[test]
fn disabled_output_rejects_targets_without_preparing_them() {
    let mut service = service(true);

    assert_eq!(
        write(&mut service, 0, &[100, 200]),
        Err(NackCode::InvalidState),
    );
    assert_eq!(accepted_angles(&service), [0, 0]);
    assert_eq!(service.status(), CALIBRATION_VALID);
}

#[test]
fn partial_initial_batch_is_rejected() {
    let mut service = service(true);
    service.set_enabled(true).unwrap();

    assert_eq!(
        write(&mut service, 0, &[100]),
        Err(NackCode::InvalidState),
    );
    assert_eq!(
        write(&mut service, 1, &[200]),
        Err(NackCode::InvalidState),
    );

    assert_eq!(accepted_angles(&service), [0, 0]);
    assert_eq!(service.output.inner.start_calls, 0);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED
    );
}

#[test]
fn complete_initial_batch_starts_output_once() {
    let service = running();

    assert_eq!(accepted_angles(&service), [100, 200]);
    assert_eq!(service.output.inner.start_calls, 1);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
}

#[test]
fn configured_servo_count_is_checked_before_backend_access() {
    let mut service = service(true);
    service.set_enabled(true).unwrap();

    // FakeOutput supports 18 IDs, but this service is configured for 2.
    assert_eq!(
        write(&mut service, 2, &[100]),
        Err(NackCode::InvalidValue),
    );
    assert_eq!(
        write(&mut service, 0, &[100, 200, 300]),
        Err(NackCode::InvalidValue),
    );
    assert_eq!(accepted_angles(&service), [0, 0]);
    assert_eq!(
        service.output.inner.targets[2].as_centidegrees(),
        0
    );
    assert_eq!(service.output.inner.start_calls, 0);
}

#[test]
fn invalid_initial_angle_rejects_entire_batch() {
    let mut service = service(true);
    service.set_enabled(true).unwrap();

    assert_eq!(
        write(&mut service, 0, &[100, 9_001]),
        Err(NackCode::InvalidValue),
    );
    assert_eq!(accepted_angles(&service), [0, 0]);
    assert_eq!(service.output.inner.start_calls, 0);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED
    );
}

#[test]
fn running_partial_update_preserves_other_targets() {
    let mut service = running();

    assert_eq!(write(&mut service, 1, &[-300]), Ok(()));

    assert_eq!(accepted_angles(&service), [100, -300]);
    assert_eq!(service.output.inner.start_calls, 1);
    assert_eq!(service.output.inner.stop_calls, 0);
}

#[test]
fn invalid_running_batch_preserves_every_target() {
    let mut service = running();

    assert_eq!(
        write(&mut service, 0, &[300, 9_001]),
        Err(NackCode::InvalidValue),
    );
    assert_eq!(accepted_angles(&service), [100, 200]);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
}

#[test]
fn busy_preserves_targets_permission_and_running_state() {
    let mut service = running();
    service.output.busy = true;

    assert_eq!(
        write(&mut service, 0, &[300, 400]),
        Err(NackCode::Busy),
    );
    assert_eq!(accepted_angles(&service), [100, 200]);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
    assert_eq!(service.output.inner.stop_calls, 0);

    service.output.busy = false;
    assert_eq!(
        write(&mut service, 0, &[300, 400]),
        Ok(())
    );
    assert_eq!(accepted_angles(&service), [300, 400]);
}

#[test]
fn technical_submission_error_latches_fault_and_stops_output() {
    let mut service = running();
    service.output.inner.target_error = true;

    assert_eq!(
        write(&mut service, 0, &[300, 400]),
        Err(NackCode::OutputError),
    );
    assert_eq!(accepted_angles(&service), [100, 200]);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR
    );
    assert_eq!(service.output.inner.stop_calls, 1);

    service.output.inner.target_error = false;

    assert_eq!(
        write(&mut service, 0, &[300, 400]),
        Err(NackCode::OutputError),
    );
    assert_eq!(accepted_angles(&service), [100, 200]);
}

#[test]
fn failed_emergency_stop_preserves_actual_running_status() {
    let mut service = running();
    service.output.inner.target_error = true;
    service.output.inner.stop_error = true;
    service.output.inner.state_after_error = OutputState::Active;

    assert_eq!(
        write(&mut service, 0, &[300, 400]),
        Err(NackCode::OutputError),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | RUNNING | OUTPUT_ERROR,
    );
    assert_eq!(service.output.inner.stop_calls, 1);
}

#[test]
fn invalid_prepared_start_does_not_latch_technical_error() {
    let mut service = service(true);
    service.set_enabled(true).unwrap();
    service.output.inner.start_error = Some(OutputStartError::InvalidTargets);

    assert_eq!(
        write(&mut service, 0, &[100, 200]),
        Err(NackCode::InvalidValue),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED
    );
    assert_eq!(service.output.inner.start_calls, 1);
    assert_eq!(service.output.inner.stop_calls, 0);
}

#[test]
fn technical_start_failure_attempts_shutdown() {
    let mut service = service(true);
    service.set_enabled(true).unwrap();
    service.output.inner.start_error = Some(OutputStartError::Backend(()));
    service.output.inner.state_after_error = OutputState::Active;

    assert_eq!(
        write(&mut service, 0, &[100, 200]),
        Err(NackCode::OutputError),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR
    );
    assert_eq!(service.output.inner.stop_calls, 1);
}

#[test]
fn reenable_requires_new_complete_initial_batch() {
    let mut service = running();

    service.set_enabled(false).unwrap();
    service.set_enabled(true).unwrap();

    assert_eq!(
        write(&mut service, 1, &[300]),
        Err(NackCode::InvalidState),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED
    );

    assert_eq!(
        write(&mut service, 0, &[400, 500]),
        Ok(())
    );
    assert_eq!(accepted_angles(&service), [400, 500]);
    assert_eq!(service.output.inner.start_calls, 2);
}
