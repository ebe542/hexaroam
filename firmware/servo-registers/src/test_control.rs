use super::*;
use crate::test_support::FakeOutput;
use crate::{OutputStartError, OutputState};

fn control() -> ControlRegister<FakeOutput> {
    ControlRegister::new(FakeOutput::inactive(), true)
}

fn active_control() -> ControlRegister<FakeOutput> {
    let mut control = control();
    control
        .apply(Opcode::Set, OUTPUT_ENABLE)
        .unwrap();
    control
}

#[test]
fn initial_inactive_output_does_not_start_automatically() {
    let control = control();

    assert_eq!(control.value(), 0);
    assert_eq!(
        control.status(),
        STATUS_CALIBRATION_VALID
    );
    assert_eq!(control.output.start_calls, 0);
    assert_eq!(control.output.stop_calls, 0);
}

#[test]
fn initial_active_backend_is_reflected_without_another_start() {
    let mut output = FakeOutput::inactive();
    output.state = OutputState::Active;

    let control = ControlRegister::new(output, true);

    assert_eq!(control.value(), OUTPUT_ENABLE);
    assert_eq!(
        control.status(),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
    );
    assert_eq!(control.output.start_calls, 0);
}

#[test]
fn write_enable_starts_output() {
    let mut control = control();

    assert_eq!(
        control.apply(Opcode::Write, OUTPUT_ENABLE),
        Ok(())
    );
    assert_eq!(control.value(), OUTPUT_ENABLE);
    assert_eq!(control.output.start_calls, 1);
    assert_eq!(
        control.status(),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
    );
}

#[test]
fn set_enable_starts_output() {
    let mut control = control();

    assert_eq!(
        control.apply(Opcode::Set, OUTPUT_ENABLE),
        Ok(())
    );
    assert_eq!(control.value(), OUTPUT_ENABLE);
    assert_eq!(control.output.start_calls, 1);
}

#[test]
fn clear_enable_stops_output() {
    let mut control = active_control();

    assert_eq!(
        control.apply(Opcode::Clear, OUTPUT_ENABLE),
        Ok(())
    );
    assert_eq!(control.value(), 0);
    assert_eq!(
        control.status(),
        STATUS_CALIBRATION_VALID
    );
    assert_eq!(control.output.stop_calls, 1);
}

#[test]
fn writing_zero_stops_output() {
    let mut control = active_control();

    assert_eq!(control.apply(Opcode::Write, 0), Ok(()));
    assert_eq!(control.value(), 0);
    assert_eq!(control.output.stop_calls, 1);
}

#[test]
fn repeated_requests_do_not_repeat_backend_operations() {
    let mut control = active_control();

    control
        .apply(Opcode::Set, OUTPUT_ENABLE)
        .unwrap();
    control
        .apply(Opcode::Write, OUTPUT_ENABLE)
        .unwrap();
    assert_eq!(control.output.start_calls, 1);

    control
        .apply(Opcode::Clear, OUTPUT_ENABLE)
        .unwrap();
    control
        .apply(Opcode::Clear, OUTPUT_ENABLE)
        .unwrap();
    control.apply(Opcode::Write, 0).unwrap();
    assert_eq!(control.output.stop_calls, 1);
}

#[test]
fn zero_bitmasks_leave_the_requested_state_unchanged() {
    for active in [false, true] {
        let mut control = if active { active_control() } else { control() };
        let before = control.value();
        let starts = control.output.start_calls;

        control.apply(Opcode::Set, 0).unwrap();
        control.apply(Opcode::Clear, 0).unwrap();

        assert_eq!(control.value(), before);
        assert_eq!(control.output.start_calls, starts);
        assert_eq!(control.output.stop_calls, 0);
    }
}

#[test]
fn reserved_bits_reject_the_whole_operation() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        let mut control = active_control();

        assert_eq!(
            control.apply(opcode, OUTPUT_ENABLE | 0x80),
            Err(ErrorCode::InvalidValue),
        );
        assert_eq!(control.value(), OUTPUT_ENABLE);
        assert_eq!(
            control.status(),
            STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
        );
        assert_eq!(control.output.start_calls, 1);
        assert_eq!(control.output.stop_calls, 0);
    }
}

#[test]
fn read_is_not_a_control_modification() {
    let mut control = control();

    assert_eq!(
        control.apply(Opcode::Read, 0),
        Err(ErrorCode::UnsupportedAccess),
    );
    assert_eq!(control.value(), 0);
    assert_eq!(control.output.start_calls, 0);
    assert_eq!(control.output.stop_calls, 0);
}

#[test]
fn missing_calibration_prevents_start_without_backend_call() {
    let mut control = ControlRegister::new(FakeOutput::inactive(), false);

    assert_eq!(
        control.apply(Opcode::Set, OUTPUT_ENABLE),
        Err(ErrorCode::NotConfigured),
    );
    assert_eq!(control.value(), 0);
    assert_eq!(control.status(), 0);
    assert_eq!(control.output.start_calls, 0);
}

#[test]
fn invalid_initial_targets_do_not_latch_fault() {
    let mut output = FakeOutput::inactive();
    output.start_error = Some(OutputStartError::InvalidTargets);
    let mut control = ControlRegister::new(output, true);

    assert_eq!(
        control.apply(Opcode::Set, OUTPUT_ENABLE),
        Err(ErrorCode::InvalidValue),
    );
    assert_eq!(control.value(), 0);
    assert_eq!(
        control.status(),
        STATUS_CALIBRATION_VALID
    );
    assert_eq!(control.output.start_calls, 1);
}

#[test]
fn technical_start_failure_preserves_request_and_reports_real_state() {
    for state in [OutputState::Inactive, OutputState::Active] {
        let mut output = FakeOutput::inactive();
        output.start_error = Some(OutputStartError::Backend(()));
        output.state_after_error = state;
        let mut control = ControlRegister::new(output, true);

        assert_eq!(
            control.apply(Opcode::Set, OUTPUT_ENABLE),
            Err(ErrorCode::OutputError),
        );
        assert_eq!(control.value(), 0);

        let active = if state == OutputState::Active {
            STATUS_OUTPUT_ACTIVE
        } else {
            0
        };
        assert_eq!(
            control.status(),
            STATUS_CALIBRATION_VALID | STATUS_FAULT | active,
        );
    }
}

#[test]
fn technical_stop_failure_preserves_request_and_reports_real_state() {
    for state in [OutputState::Inactive, OutputState::Active] {
        let mut control = active_control();
        control.output.stop_error = true;
        control.output.state_after_error = state;

        assert_eq!(
            control.apply(Opcode::Clear, OUTPUT_ENABLE),
            Err(ErrorCode::OutputError),
        );
        assert_eq!(control.value(), OUTPUT_ENABLE);

        let active = if state == OutputState::Active {
            STATUS_OUTPUT_ACTIVE
        } else {
            0
        };
        assert_eq!(
            control.status(),
            STATUS_CALIBRATION_VALID | STATUS_FAULT | active,
        );
    }
}

#[test]
fn latched_fault_prevents_another_enable_request() {
    let mut output = FakeOutput::inactive();
    output.start_error = Some(OutputStartError::Backend(()));
    let mut control = ControlRegister::new(output, true);

    assert_eq!(
        control.apply(Opcode::Set, OUTPUT_ENABLE),
        Err(ErrorCode::OutputError),
    );

    control.output.start_error = None;

    assert_eq!(
        control.apply(Opcode::Set, OUTPUT_ENABLE),
        Err(ErrorCode::InvalidState),
    );
    assert_eq!(control.output.start_calls, 1);
    assert_eq!(
        control.status(),
        STATUS_CALIBRATION_VALID | STATUS_FAULT
    );
}

#[test]
fn stopping_in_fault_remains_possible_without_clearing_fault() {
    let mut control = active_control();
    control.output.stop_error = true;
    control.output.state_after_error = OutputState::Active;

    assert_eq!(
        control.apply(Opcode::Clear, OUTPUT_ENABLE),
        Err(ErrorCode::OutputError),
    );

    control.output.stop_error = false;

    assert_eq!(
        control.apply(Opcode::Clear, OUTPUT_ENABLE),
        Ok(())
    );
    assert_eq!(control.value(), 0);
    assert_eq!(control.output.stop_calls, 2);
    assert_eq!(
        control.status(),
        STATUS_CALIBRATION_VALID | STATUS_FAULT,
    );
}
