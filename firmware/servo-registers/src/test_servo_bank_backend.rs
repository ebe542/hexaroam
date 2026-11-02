//! Register backend integration with a real ServoBank and recording output.

extern crate std;

use self::std::{cell::RefCell, rc::Rc};

use hexaroam_servo_control::{
    Angle, Calibration, PulseBankLifecycle, PulseBankOutput, ServoBank,
    ServoId, ServoTarget,
};
use hexaroam_servo_protocol::{
    Frame, Header, Opcode, ParsedFrame, validate_header,
};
use hexaroam_servo_signal::SignalOutputState;

use crate::{
    ErrorCode, OUTPUT_ENABLE, OutputBackend, OutputStartError, OutputState,
    REGISTER_REGIONS, RegisterService, STATUS_CALIBRATION_VALID,
    STATUS_OUTPUT_ACTIVE, ServoBankBackendError, TargetReadError,
    TargetWriteError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubmissionError {
    Busy,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleError {
    AlreadyActive,
    Faulted,
    AbortTimeout,
}

#[derive(Default)]
struct Recorded {
    active: bool,
    pulses: Option<[u32; 2]>,
    start_calls: usize,
    stop_calls: usize,
    submit_calls: usize,
    service_calls: usize,
    start_failure: Option<(LifecycleError, bool)>,
    submission_failure: Option<SubmissionError>,
    stop_failure: bool,
}

struct RecordingOutput {
    recorded: Rc<RefCell<Recorded>>,
}

impl PulseBankOutput for RecordingOutput {
    type Error = SubmissionError;

    fn submit_pulses(&mut self, pulses: &[u32]) -> Result<(), Self::Error> {
        let mut recorded = self.recorded.borrow_mut();
        recorded.submit_calls += 1;

        if !recorded.active {
            return Err(SubmissionError::Stopped);
        }

        if let Some(error) = recorded.submission_failure {
            return Err(error);
        }

        recorded.pulses = Some(
            pulses
                .try_into()
                .expect("Complete two-servo batch"),
        );

        Ok(())
    }

    fn service(&mut self) {
        self.recorded.borrow_mut().service_calls += 1;
    }
}

impl PulseBankLifecycle for RecordingOutput {
    type LifecycleError = LifecycleError;

    fn start_pulses(
        &mut self,
        pulses: &[u32],
    ) -> Result<(), Self::LifecycleError> {
        let mut recorded = self.recorded.borrow_mut();
        recorded.start_calls += 1;

        if let Some((error, active_after_error)) = recorded.start_failure {
            recorded.active = active_after_error;
            return Err(error);
        }

        if recorded.active {
            return Err(LifecycleError::AlreadyActive);
        }

        recorded.pulses = Some(
            pulses
                .try_into()
                .expect("Complete two-servo startup"),
        );
        recorded.active = true;

        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::LifecycleError> {
        let mut recorded = self.recorded.borrow_mut();
        recorded.stop_calls += 1;

        if !recorded.active {
            return Ok(());
        }

        if recorded.stop_failure {
            return Err(LifecycleError::AbortTimeout);
        }

        recorded.active = false;
        Ok(())
    }

    fn state(&self) -> SignalOutputState {
        if self.recorded.borrow().active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        }
    }
}

type TestBank = ServoBank<RecordingOutput, 2>;
type TestService = RegisterService<TestBank>;

fn angle(value: i32) -> Angle {
    Angle::from_centidegrees(value)
}

fn target(id: usize, value: i32) -> ServoTarget {
    ServoTarget {
        id: ServoId(id),
        angle: angle(value),
    }
}

fn calibration(lower: u32, upper: u32) -> Calibration {
    Calibration::new(
        angle(-4_500),
        lower,
        angle(4_500),
        upper,
    )
    .expect("Valid synthetic calibration")
}

fn bank() -> (TestBank, Rc<RefCell<Recorded>>) {
    let recorded = Rc::new(RefCell::new(Recorded::default()));
    let output = RecordingOutput {
        recorded: Rc::clone(&recorded),
    };

    (
        ServoBank::new(
            [calibration(1_000, 2_000), calibration(1_200, 1_800)],
            output,
        ),
        recorded,
    )
}

fn prepared_bank() -> (TestBank, Rc<RefCell<Recorded>>) {
    let (mut bank, recorded) = bank();

    OutputBackend::set_targets(
        &mut bank,
        &[target(0, -4_500), target(1, 4_500)],
    )
    .expect("Prepare initial targets");

    (bank, recorded)
}

fn assert_targets(bank: &TestBank, first: i32, second: i32) {
    assert_eq!(
        OutputBackend::target(bank, ServoId(0)),
        Ok(angle(first)),
    );
    assert_eq!(
        OutputBackend::target(bank, ServoId(1)),
        Ok(angle(second)),
    );
}

fn request(
    opcode: Opcode,
    index: u8,
    length: u16,
    payload: &[u8],
) -> ParsedFrame<'_> {
    let header = Header {
        opcode,
        index,
        length,
    };

    ParsedFrame {
        frame: Frame { header, payload },
        validation: validate_header(&header, &REGISTER_REGIONS),
    }
}

fn modify(
    service: &mut TestService,
    opcode: Opcode,
    index: u8,
    payload: &[u8],
) {
    let mut output = [0xA5; 8];

    assert_eq!(
        service
            .handle(
                request(
                    opcode,
                    index,
                    payload.len() as u16,
                    payload
                ),
                &mut output,
            )
            .unwrap(),
        None,
    );
    assert_eq!(output, [0xA5; 8]);
}

fn assert_read(
    service: &mut TestService,
    requested_index: u8,
    length: u16,
    response_index: u8,
    expected: &[u8],
) {
    let mut output = [0; 8];

    let response = service
        .handle(
            request(
                Opcode::Read,
                requested_index,
                length,
                &[],
            ),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        response.header,
        Header {
            opcode: Opcode::Read,
            index: response_index,
            length: expected.len() as u16,
        },
    );
    assert_eq!(response.payload, expected);
}

#[test]
fn initial_state_and_target_errors_are_reported_without_actions() {
    let (bank, recorded) = bank();

    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Inactive
    );
    assert_eq!(
        OutputBackend::target(&bank, ServoId(0)),
        Err(TargetReadError::UninitializedServo(
            ServoId(0)
        )),
    );
    assert_eq!(
        OutputBackend::target(&bank, ServoId(2)),
        Err(TargetReadError::UnknownServo(ServoId(
            2
        ))),
    );

    // State must reflect the backend rather than a cached adapter value.
    recorded.borrow_mut().active = true;
    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Active
    );

    let recorded = recorded.borrow();
    assert_eq!(recorded.start_calls, 0);
    assert_eq!(recorded.stop_calls, 0);
    assert_eq!(recorded.submit_calls, 0);
    assert_eq!(recorded.pulses, None);
}

#[test]
fn inactive_partial_writes_prepare_without_output() {
    let (mut bank, recorded) = bank();

    assert_eq!(
        OutputBackend::set_targets(&mut bank, &[target(0, -4_500)]),
        Ok(()),
    );
    assert_eq!(
        OutputBackend::target(&bank, ServoId(0)),
        Ok(angle(-4_500)),
    );
    assert_eq!(
        OutputBackend::target(&bank, ServoId(1)),
        Err(TargetReadError::UninitializedServo(
            ServoId(1)
        )),
    );

    assert_eq!(
        OutputBackend::set_targets(&mut bank, &[target(1, 4_500)]),
        Ok(()),
    );
    assert_targets(&bank, -4_500, 4_500);

    let recorded = recorded.borrow();
    assert!(!recorded.active);
    assert_eq!(recorded.start_calls, 0);
    assert_eq!(recorded.submit_calls, 0);
    assert_eq!(recorded.pulses, None);
}

#[test]
fn start_uses_complete_calibrated_pulses() {
    let (mut bank, recorded) = prepared_bank();

    assert_eq!(OutputBackend::start(&mut bank), Ok(()));
    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Active
    );
    assert_targets(&bank, -4_500, 4_500);

    let recorded = recorded.borrow();
    assert_eq!(recorded.start_calls, 1);
    assert_eq!(recorded.submit_calls, 0);
    assert_eq!(recorded.pulses, Some([1_000, 1_800]));
}

#[test]
fn incomplete_start_is_invalid_targets_without_backend_call() {
    let (mut bank, recorded) = bank();

    assert_eq!(
        OutputBackend::start(&mut bank),
        Err(OutputStartError::InvalidTargets),
    );

    OutputBackend::set_targets(&mut bank, &[target(0, 0)]).unwrap();

    assert_eq!(
        OutputBackend::start(&mut bank),
        Err(OutputStartError::InvalidTargets),
    );
    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Inactive
    );
    assert_eq!(recorded.borrow().start_calls, 0);
    assert_eq!(recorded.borrow().pulses, None);
}

#[test]
fn active_partial_write_submits_complete_batch() {
    let (mut bank, recorded) = prepared_bank();
    OutputBackend::start(&mut bank).unwrap();

    assert_eq!(
        OutputBackend::set_targets(&mut bank, &[target(0, 0)]),
        Ok(()),
    );

    assert_targets(&bank, 0, 4_500);
    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Active
    );

    let recorded = recorded.borrow();
    assert_eq!(recorded.start_calls, 1);
    assert_eq!(recorded.submit_calls, 1);
    assert_eq!(recorded.pulses, Some([1_500, 1_800]));
}

#[test]
fn invalid_writes_preserve_all_targets_in_both_states() {
    for active in [false, true] {
        let (mut bank, recorded) = prepared_bank();

        if active {
            OutputBackend::start(&mut bank).unwrap();
        }

        assert_eq!(
            OutputBackend::set_targets(
                &mut bank,
                &[target(0, 0), target(2, 0)],
            ),
            Err(TargetWriteError::UnknownServo(ServoId(
                2
            ))),
        );
        assert_targets(&bank, -4_500, 4_500);

        assert_eq!(
            OutputBackend::set_targets(
                &mut bank,
                &[target(0, 0), target(0, 100)],
            ),
            Err(TargetWriteError::DuplicateServo(
                ServoId(0)
            )),
        );
        assert_targets(&bank, -4_500, 4_500);

        assert_eq!(
            OutputBackend::set_targets(
                &mut bank,
                &[target(0, 0), target(1, 4_501)],
            ),
            Err(TargetWriteError::InvalidAngle(ServoId(
                1
            ))),
        );
        assert_targets(&bank, -4_500, 4_500);

        let recorded = recorded.borrow();
        assert_eq!(recorded.active, active);
        assert_eq!(recorded.submit_calls, 0);
        assert_eq!(
            recorded.pulses,
            if active { Some([1_000, 1_800]) } else { None },
        );
    }
}

#[test]
fn active_incomplete_write_reports_uninitialized_servo() {
    let (mut bank, recorded) = bank();

    // Simulate an already active driver whose bank has no targets yet.
    recorded.borrow_mut().active = true;

    assert_eq!(
        OutputBackend::set_targets(&mut bank, &[target(0, 0)]),
        Err(TargetWriteError::UninitializedServo(
            ServoId(1)
        )),
    );
    assert_eq!(
        OutputBackend::target(&bank, ServoId(0)),
        Err(TargetReadError::UninitializedServo(
            ServoId(0)
        )),
    );
    assert_eq!(recorded.borrow().submit_calls, 0);
    assert_eq!(recorded.borrow().pulses, None);
}

#[test]
fn start_failure_preserves_lifecycle_error_and_backend_state() {
    for active_after_error in [false, true] {
        let (mut bank, recorded) = prepared_bank();
        recorded.borrow_mut().start_failure = Some((
            LifecycleError::Faulted,
            active_after_error,
        ));

        assert_eq!(
            OutputBackend::start(&mut bank),
            Err(OutputStartError::Backend(
                ServoBankBackendError::Lifecycle(LifecycleError::Faulted),
            )),
        );

        let expected = if active_after_error {
            OutputState::Active
        } else {
            OutputState::Inactive
        };

        assert_eq!(OutputBackend::state(&bank), expected);
        assert_targets(&bank, -4_500, 4_500);
        assert_eq!(recorded.borrow().start_calls, 1);
        assert_eq!(recorded.borrow().submit_calls, 0);
        assert_eq!(recorded.borrow().pulses, None);
    }
}

#[test]
fn submission_failure_preserves_targets_and_previous_pulses() {
    let (mut bank, recorded) = prepared_bank();
    OutputBackend::start(&mut bank).unwrap();

    recorded.borrow_mut().submission_failure = Some(SubmissionError::Busy);

    assert_eq!(
        OutputBackend::set_targets(&mut bank, &[target(0, 0)]),
        Err(TargetWriteError::Backend(
            ServoBankBackendError::Submission(SubmissionError::Busy),
        )),
    );

    assert_targets(&bank, -4_500, 4_500);
    assert_eq!(recorded.borrow().submit_calls, 1);
    assert_eq!(
        recorded.borrow().pulses,
        Some([1_000, 1_800])
    );
    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Active
    );
}

#[test]
fn stop_failure_retry_and_restart_preserve_targets() {
    let (mut bank, recorded) = prepared_bank();
    OutputBackend::start(&mut bank).unwrap();

    recorded.borrow_mut().stop_failure = true;

    assert_eq!(
        OutputBackend::stop(&mut bank),
        Err(ServoBankBackendError::Lifecycle(
            LifecycleError::AbortTimeout,
        )),
    );
    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Active
    );
    assert_targets(&bank, -4_500, 4_500);
    assert_eq!(recorded.borrow().stop_calls, 1);

    recorded.borrow_mut().stop_failure = false;

    assert_eq!(OutputBackend::stop(&mut bank), Ok(()));
    assert_eq!(OutputBackend::stop(&mut bank), Ok(()));
    assert_eq!(
        OutputBackend::state(&bank),
        OutputState::Inactive
    );
    assert_targets(&bank, -4_500, 4_500);

    OutputBackend::set_targets(&mut bank, &[target(0, 0)]).unwrap();
    OutputBackend::start(&mut bank).unwrap();

    assert_targets(&bank, 0, 4_500);
    assert_eq!(recorded.borrow().start_calls, 2);
    assert_eq!(recorded.borrow().stop_calls, 3);
    assert_eq!(recorded.borrow().submit_calls, 0);
    assert_eq!(
        recorded.borrow().pulses,
        Some([1_500, 1_800])
    );
}

#[test]
fn register_service_controls_bank_and_forwards_service() {
    let (bank, recorded) = bank();
    let mut service = RegisterService::new(1, 2, bank, true);

    service.service();
    assert_eq!(recorded.borrow().service_calls, 1);
    assert_eq!(recorded.borrow().start_calls, 0);
    assert_eq!(recorded.borrow().submit_calls, 0);

    let mut targets = [0; 8];
    targets[..4].copy_from_slice(&(-4_500_i32).to_le_bytes());
    targets[4..].copy_from_slice(&4_500_i32.to_le_bytes());

    modify(
        &mut service,
        Opcode::Write,
        0x20,
        &targets,
    );
    assert_eq!(recorded.borrow().pulses, None);
    assert_read(&mut service, 0x20, 8, 0x20, &targets);

    modify(
        &mut service,
        Opcode::Set,
        3,
        &[OUTPUT_ENABLE],
    );
    assert_eq!(
        recorded.borrow().pulses,
        Some([1_000, 1_800])
    );

    service.service();
    assert_eq!(recorded.borrow().service_calls, 2);
    assert_eq!(recorded.borrow().start_calls, 1);

    modify(
        &mut service,
        Opcode::Write,
        0x20,
        &0_i32.to_le_bytes(),
    );
    assert_eq!(
        recorded.borrow().pulses,
        Some([1_500, 1_800])
    );

    modify(
        &mut service,
        Opcode::Clear,
        3,
        &[OUTPUT_ENABLE],
    );
    assert!(!recorded.borrow().active);
    assert_eq!(recorded.borrow().stop_calls, 1);

    service.service();
    assert_eq!(recorded.borrow().service_calls, 3);
    assert_eq!(recorded.borrow().start_calls, 1);
    assert_eq!(recorded.borrow().submit_calls, 1);

    targets[..4].copy_from_slice(&0_i32.to_le_bytes());
    assert_read(&mut service, 0x20, 8, 0x20, &targets);
    assert_read(
        &mut service,
        1,
        1,
        1,
        &[STATUS_CALIBRATION_VALID],
    );

    modify(
        &mut service,
        Opcode::Set,
        3,
        &[OUTPUT_ENABLE],
    );

    assert_read(
        &mut service,
        1,
        1,
        1,
        &[STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE],
    );
    assert_eq!(recorded.borrow().start_calls, 2);
}

#[test]
fn uninitialized_register_access_reports_state_error_without_fault() {
    let (bank, recorded) = bank();
    let mut service = RegisterService::new(1, 2, bank, true);

    assert_read(
        &mut service,
        0x20,
        4,
        2,
        &(ErrorCode::InvalidState as u32).to_le_bytes(),
    );

    // A rejected READ must not trigger a technical Fault.
    assert_read(
        &mut service,
        1,
        1,
        1,
        &[STATUS_CALIBRATION_VALID],
    );

    modify(
        &mut service,
        Opcode::Write,
        0x20,
        &0_i32.to_le_bytes(),
    );

    // A batch containing the missing second target returns an error,
    // rather than exposing a partial target response.
    assert_read(
        &mut service,
        0x20,
        8,
        2,
        &(ErrorCode::InvalidState as u32).to_le_bytes(),
    );
    assert_read(
        &mut service,
        0x20,
        4,
        0x20,
        &0_i32.to_le_bytes(),
    );

    assert_eq!(recorded.borrow().start_calls, 0);
    assert_eq!(recorded.borrow().submit_calls, 0);
    assert_eq!(recorded.borrow().pulses, None);
}
