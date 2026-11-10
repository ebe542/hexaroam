use core::cell::Cell;

use super::process;
use crate::control::{
    BackendError, CalibrationControl, OUTPUT_ERROR, PULSE_READY, PulseBackend,
    RUNNING,
};
use hexaroam_servo_signal::{PulseLimits, SignalOutputState};

struct FakeBackend<'a> {
    active: bool,
    error: &'a Cell<Option<BackendError>>,
    calls: &'a Cell<usize>,
}

impl FakeBackend<'_> {
    fn operation(&self) -> Result<(), BackendError> {
        self.calls.set(self.calls.get() + 1);
        match self.error.get() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl PulseBackend for FakeBackend<'_> {
    fn state(&self) -> SignalOutputState {
        if self.active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        }
    }

    fn is_faulted(&self) -> bool {
        false
    }

    fn start(&mut self, _: u32) -> Result<(), BackendError> {
        self.operation()?;
        self.active = true;
        Ok(())
    }

    fn set_pulse(&mut self, _: u32) -> Result<(), BackendError> {
        self.operation()
    }

    fn stop(&mut self) -> Result<(), BackendError> {
        self.operation()?;
        self.active = false;
        Ok(())
    }

    fn service(&mut self) {}
}

fn control<'a>(
    error: &'a Cell<Option<BackendError>>,
    calls: &'a Cell<usize>,
) -> CalibrationControl<FakeBackend<'a>> {
    CalibrationControl::new(
        FakeBackend {
            active: false,
            error,
            calls,
        },
        PulseLimits::new(1_000, 2_000).unwrap(),
    )
}

fn assert_reply<B: PulseBackend>(
    control: &mut CalibrationControl<B>,
    request: &[u8],
    expected: &[u8],
) {
    let response = process(control, request).expect("Expected response");
    assert_eq!(response.as_bytes(), expected);
}

fn assert_status<B: PulseBackend>(
    control: &mut CalibrationControl<B>,
    flags: u32,
    pulse: u32,
) {
    let mut expected = [0x80, 0x01, 0, 0, 0, 0, 0, 0, 0, 0];
    expected[2..6].copy_from_slice(&flags.to_le_bytes());
    expected[6..10].copy_from_slice(&pulse.to_le_bytes());
    assert_reply(control, &[0x01], &expected);
}

fn prepare<B: PulseBackend>(control: &mut CalibrationControl<B>) {
    assert_reply(
        control,
        &[0x03, 0xDC, 0x05, 0, 0],
        &[0x80, 0x03],
    );
}

fn start<B: PulseBackend>(control: &mut CalibrationControl<B>) {
    prepare(control);
    assert_reply(control, &[0x02, 1], &[0x80, 0x02]);
}

#[test]
fn initial_status_reports_no_prepared_pulse() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    assert_status(&mut control, 0, 0);
    assert_eq!(calls.get(), 0);
}

#[test]
fn preparing_pulse_returns_ack_without_starting_output() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    prepare(&mut control);

    assert_status(&mut control, PULSE_READY, 1_500);
    assert_eq!(calls.get(), 0);
}

#[test]
fn enable_without_pulse_returns_invalid_state() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    assert_reply(
        &mut control,
        &[0x02, 1],
        &[0x81, 0x02, 0x04],
    );
    assert_status(&mut control, 0, 0);
    assert_eq!(calls.get(), 0);
}

#[test]
fn enabling_prepared_output_returns_ack_and_running_status() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    start(&mut control);

    assert_status(
        &mut control,
        PULSE_READY | RUNNING,
        1_500,
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn disable_returns_ack_and_preserves_prepared_pulse() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    start(&mut control);
    assert_reply(&mut control, &[0x02, 0], &[0x80, 0x02]);

    assert_status(&mut control, PULSE_READY, 1_500);
    assert_eq!(calls.get(), 2);
}

#[test]
fn active_pulse_update_returns_ack() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    start(&mut control);
    assert_reply(
        &mut control,
        &[0x03, 0xA4, 0x06, 0, 0],
        &[0x80, 0x03],
    );

    assert_status(
        &mut control,
        PULSE_READY | RUNNING,
        1_700,
    );
    assert_eq!(calls.get(), 2);
}

#[test]
fn out_of_range_returns_invalid_value_and_preserves_pulse() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    prepare(&mut control);
    assert_reply(
        &mut control,
        &[0x03, 0xD1, 0x07, 0, 0],
        &[0x81, 0x03, 0x03],
    );

    assert_status(&mut control, PULSE_READY, 1_500);
    assert_eq!(calls.get(), 0);
}

#[test]
fn busy_returns_nack_without_changing_status_or_pulse() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    start(&mut control);
    error.set(Some(BackendError::Busy));

    assert_reply(
        &mut control,
        &[0x03, 0xA4, 0x06, 0, 0],
        &[0x81, 0x03, 0x05],
    );
    assert_status(
        &mut control,
        PULSE_READY | RUNNING,
        1_500,
    );
    assert_eq!(calls.get(), 2);
}

#[test]
fn output_error_is_reported_and_latched_but_stop_remains_possible() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    start(&mut control);
    error.set(Some(BackendError::OutputError));

    assert_reply(
        &mut control,
        &[0x03, 0xA4, 0x06, 0, 0],
        &[0x81, 0x03, 0x06],
    );
    assert_status(
        &mut control,
        PULSE_READY | RUNNING | OUTPUT_ERROR,
        1_500,
    );

    error.set(None);
    assert_reply(&mut control, &[0x02, 0], &[0x80, 0x02]);
    assert_reply(
        &mut control,
        &[0x02, 1],
        &[0x81, 0x02, 0x06],
    );

    assert_status(
        &mut control,
        PULSE_READY | OUTPUT_ERROR,
        1_500,
    );
    assert_eq!(calls.get(), 3);
}

#[test]
fn malformed_requests_return_invalid_length_without_side_effects() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    for request in [
        &[0x01, 0][..],
        &[0x02][..],
        &[0x02, 1, 0][..],
        &[0x03, 0, 0, 0][..],
        &[0x03, 0, 0, 0, 0, 0][..],
    ] {
        assert_reply(
            &mut control,
            request,
            &[0x81, request[0], 0x02],
        );
    }

    assert_status(&mut control, 0, 0);
    assert_eq!(calls.get(), 0);
}

#[test]
fn invalid_enable_value_returns_invalid_value() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    assert_reply(
        &mut control,
        &[0x02, 2],
        &[0x81, 0x02, 0x03],
    );

    assert_status(&mut control, 0, 0);
    assert_eq!(calls.get(), 0);
}

#[test]
fn unknown_command_returns_original_command_in_nack() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    assert_reply(
        &mut control,
        &[0x7F],
        &[0x81, 0x7F, 0x01],
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn incoming_responses_are_ignored() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    for content in [
        &[0x80][..],
        &[0x81][..],
        &[0x80, 0x03][..],
        &[0x81, 0x03, 0x05][..],
    ] {
        assert!(process(&mut control, content).is_none());
    }

    assert_eq!(control.status(), 0);
    assert_eq!(calls.get(), 0);
}

#[test]
fn empty_content_is_ignored() {
    let error = Cell::new(None);
    let calls = Cell::new(0);
    let mut control = control(&error, &calls);

    assert!(process(&mut control, &[]).is_none());
    assert_eq!(control.status(), 0);
    assert_eq!(calls.get(), 0);
}
