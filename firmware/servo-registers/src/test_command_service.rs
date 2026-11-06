use super::{
    CALIBRATION_VALID, CommandService, OUTPUT_ENABLED, OUTPUT_ERROR, RUNNING,
};
use crate::{OutputState, test_support::FakeOutput};
use hexaroam_servo_protocol::command::NackCode;

fn service(calibration_valid: bool) -> CommandService<FakeOutput, 18> {
    CommandService::new(
        FakeOutput::inactive(),
        calibration_valid,
    )
}

#[test]
fn valid_boot_is_calibrated_but_disabled_and_inactive() {
    let service = service(true);

    assert_eq!(service.status(), CALIBRATION_VALID);
    assert_eq!(service.output.start_calls, 0);
    assert_eq!(service.output.stop_calls, 0);
}

#[test]
fn invalid_calibration_boot_has_no_status_flags() {
    let service = service(false);

    assert_eq!(service.status(), 0);
}

#[test]
fn enable_without_calibration_is_rejected_without_output_access() {
    let mut service = service(false);

    assert_eq!(
        service.set_enabled(true),
        Err(NackCode::InvalidState),
    );
    assert_eq!(service.status(), 0);
    assert_eq!(service.output.start_calls, 0);
    assert_eq!(service.output.stop_calls, 0);
}

#[test]
fn enable_grants_permission_without_starting_output() {
    let mut service = service(true);

    assert_eq!(service.set_enabled(true), Ok(()));
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED,
    );
    assert_eq!(service.output.start_calls, 0);
    assert_eq!(service.output.stop_calls, 0);
}

#[test]
fn repeated_enable_preserves_running_output() {
    let mut service = service(true);

    service.set_enabled(true).unwrap();

    // Simulate the backend having started after an initial target set.
    service.output.state = OutputState::Active;

    assert_eq!(service.set_enabled(true), Ok(()));
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
    assert_eq!(service.output.start_calls, 0);
    assert_eq!(service.output.stop_calls, 0);
}

#[test]
fn repeated_disable_of_inactive_output_does_not_call_backend() {
    let mut service = service(true);

    service.set_enabled(true).unwrap();

    assert_eq!(service.set_enabled(false), Ok(()));
    assert_eq!(service.set_enabled(false), Ok(()));
    assert_eq!(service.status(), CALIBRATION_VALID);
    assert_eq!(service.output.stop_calls, 0);
}

#[test]
fn disable_remains_available_without_calibration() {
    let mut service = service(false);

    assert_eq!(service.set_enabled(false), Ok(()));
    assert_eq!(service.status(), 0);
}

#[test]
fn disable_stops_output_and_clears_permission() {
    let mut service = service(true);

    service.set_enabled(true).unwrap();
    service.output.state = OutputState::Active;

    assert_eq!(service.set_enabled(false), Ok(()));
    assert_eq!(service.status(), CALIBRATION_VALID);
    assert_eq!(service.output.stop_calls, 1);
}

#[test]
fn failed_stop_revokes_permission_but_reports_still_active_output() {
    let mut service = service(true);

    service.set_enabled(true).unwrap();
    service.output.state = OutputState::Active;
    service.output.stop_error = true;
    service.output.state_after_error = OutputState::Active;

    assert_eq!(
        service.set_enabled(false),
        Err(NackCode::OutputError),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | RUNNING | OUTPUT_ERROR,
    );
    assert_eq!(service.output.stop_calls, 1);
}

#[test]
fn failed_stop_reports_inactive_state_when_backend_confirms_it() {
    let mut service = service(true);

    service.set_enabled(true).unwrap();
    service.output.state = OutputState::Active;
    service.output.stop_error = true;
    service.output.state_after_error = OutputState::Inactive;

    assert_eq!(
        service.set_enabled(false),
        Err(NackCode::OutputError),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR,
    );
}

#[test]
fn later_successful_stop_does_not_clear_latched_error() {
    let mut service = service(true);

    service.set_enabled(true).unwrap();
    service.output.state = OutputState::Active;
    service.output.stop_error = true;
    service.output.state_after_error = OutputState::Active;

    assert_eq!(
        service.set_enabled(false),
        Err(NackCode::OutputError),
    );

    service.output.stop_error = false;

    assert_eq!(service.set_enabled(false), Ok(()));
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR,
    );
    assert_eq!(service.output.stop_calls, 2);
}

#[test]
fn latched_output_error_prevents_enable() {
    let mut service = service(true);

    service.set_enabled(true).unwrap();
    service.output.state = OutputState::Active;
    service.output.stop_error = true;
    service.output.state_after_error = OutputState::Inactive;

    assert_eq!(
        service.set_enabled(false),
        Err(NackCode::OutputError),
    );

    assert_eq!(
        service.set_enabled(true),
        Err(NackCode::OutputError),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR,
    );
    assert_eq!(service.output.start_calls, 0);
}
