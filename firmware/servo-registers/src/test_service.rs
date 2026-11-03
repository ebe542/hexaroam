use super::*;
use crate::test_support::FakeOutput;
use crate::{
    OUTPUT_ENABLE, OutputStartError, OutputState, STATUS_CALIBRATION_VALID,
    STATUS_FAULT, STATUS_OUTPUT_ACTIVE,
};

fn service() -> RegisterService<FakeOutput> {
    RegisterService::new(0x1234, 18, FakeOutput::inactive(), true)
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

fn assert_reply(
    service: &mut RegisterService<FakeOutput>,
    request: ParsedFrame<'_>,
    opcode: Opcode,
    expected: &[u8],
) {
    let index = request.frame.header.index;
    let mut output = [0xA5; 8];

    let response = service
        .handle(request, &mut output)
        .unwrap()
        .expect("Every request must receive a response");

    assert_eq!(
        response.header,
        Header {
            opcode,
            index,
            length: expected.len() as u16,
        },
    );
    assert_eq!(response.payload, expected);
    assert!(
        output[expected.len()..]
            .iter()
            .all(|&byte| byte == 0xA5)
    );
}

fn read_u8(service: &mut RegisterService<FakeOutput>, index: u8) -> u8 {
    let mut output = [0; 1];
    let response = service
        .handle(
            request(Opcode::Read, index, 1, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        response.header,
        Header {
            opcode: Opcode::Ack,
            index,
            length: 1,
        },
    );

    response.payload[0]
}

fn modify_control(
    service: &mut RegisterService<FakeOutput>,
    opcode: Opcode,
    value: u8,
    error: Option<ErrorCode>,
) {
    let payload = [value];
    let request = request(opcode, 3, 1, &payload);

    match error {
        None => assert_reply(service, request, Opcode::Ack, &[]),
        Some(error) => {
            assert_reply(
                service,
                request,
                Opcode::Nack,
                &[error as u8],
            );
        }
    }
}

#[test]
fn successful_reads_return_ack_with_register_data() {
    let mut service = service();

    assert_reply(
        &mut service,
        request(Opcode::Read, 0, 2, &[]),
        Opcode::Ack,
        &[0x34, 0x12],
    );
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(read_u8(&mut service, 4), 18);
}

#[test]
fn rejected_requests_return_nack_with_original_index() {
    for (opcode, index, length, payload, error) in [
        (
            Opcode::Read,
            0xFF,
            1,
            &[][..],
            ErrorCode::UnknownIndex,
        ),
        (
            Opcode::Write,
            0xFF,
            1,
            &[0][..],
            ErrorCode::UnknownIndex,
        ),
        (
            Opcode::Set,
            0xFF,
            1,
            &[0][..],
            ErrorCode::UnknownIndex,
        ),
        (
            Opcode::Clear,
            0xFF,
            1,
            &[0][..],
            ErrorCode::UnknownIndex,
        ),
        (
            Opcode::Write,
            1,
            1,
            &[0][..],
            ErrorCode::UnsupportedAccess,
        ),
        (
            Opcode::Set,
            1,
            1,
            &[0][..],
            ErrorCode::UnsupportedAccess,
        ),
        (
            Opcode::Clear,
            1,
            1,
            &[0][..],
            ErrorCode::UnsupportedAccess,
        ),
        (
            Opcode::Read,
            0,
            1,
            &[][..],
            ErrorCode::InvalidLength,
        ),
        (
            Opcode::Read,
            1,
            2,
            &[][..],
            ErrorCode::OutOfBounds,
        ),
        (
            Opcode::Read,
            1,
            1,
            &[0][..],
            ErrorCode::InvalidLength,
        ),
        (
            Opcode::Write,
            3,
            1,
            &[][..],
            ErrorCode::InvalidLength,
        ),
        (
            Opcode::Write,
            3,
            2,
            &[1, 0][..],
            ErrorCode::OutOfBounds,
        ),
    ] {
        let mut service = service();

        assert_reply(
            &mut service,
            request(opcode, index, length, payload),
            Opcode::Nack,
            &[error as u8],
        );
        assert_eq!(read_u8(&mut service, 3), 0);
        assert_eq!(
            read_u8(&mut service, 1),
            STATUS_CALIBRATION_VALID,
        );
    }
}

#[test]
fn former_last_error_index_is_unassigned() {
    for (opcode, payload) in [
        (Opcode::Read, &[][..]),
        (Opcode::Write, &[0; 4][..]),
        (Opcode::Set, &[0; 4][..]),
        (Opcode::Clear, &[0; 4][..]),
    ] {
        let mut service = service();

        assert_reply(
            &mut service,
            request(opcode, 2, 4, payload),
            Opcode::Nack,
            &[ErrorCode::UnknownIndex as u8],
        );
    }
}

#[test]
fn successful_request_after_rejection_has_independent_result() {
    let mut service = service();

    assert_reply(
        &mut service,
        request(Opcode::Write, 0xFF, 1, &[0]),
        Opcode::Nack,
        &[ErrorCode::UnknownIndex as u8],
    );
    modify_control(
        &mut service,
        Opcode::Set,
        OUTPUT_ENABLE,
        None,
    );

    assert_reply(
        &mut service,
        request(Opcode::Read, 0, 2, &[]),
        Opcode::Ack,
        &[0x34, 0x12],
    );
    assert_eq!(read_u8(&mut service, 3), OUTPUT_ENABLE);
}

#[test]
fn successful_control_operations_return_empty_ack() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        modify_control(&mut service(), opcode, 0, None);
    }
}

#[test]
fn control_set_and_clear_update_control_and_status() {
    let mut service = service();

    modify_control(
        &mut service,
        Opcode::Set,
        OUTPUT_ENABLE,
        None,
    );
    assert_eq!(read_u8(&mut service, 3), OUTPUT_ENABLE);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
    );

    modify_control(
        &mut service,
        Opcode::Clear,
        OUTPUT_ENABLE,
        None,
    );
    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn control_write_can_enable_and_disable_output() {
    let mut service = service();

    modify_control(
        &mut service,
        Opcode::Write,
        OUTPUT_ENABLE,
        None,
    );
    assert_eq!(read_u8(&mut service, 3), OUTPUT_ENABLE);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
    );

    modify_control(&mut service, Opcode::Write, 0, None);
    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn reserved_control_bits_return_nack_without_state_change() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        let mut service = service();

        modify_control(
            &mut service,
            opcode,
            0x80,
            Some(ErrorCode::InvalidValue),
        );

        assert_eq!(read_u8(&mut service, 3), 0);
        assert_eq!(
            read_u8(&mut service, 1),
            STATUS_CALIBRATION_VALID,
        );
    }
}

#[test]
fn enabling_without_calibration_returns_nack() {
    let mut service =
        RegisterService::new(1, 18, FakeOutput::inactive(), false);

    modify_control(
        &mut service,
        Opcode::Set,
        OUTPUT_ENABLE,
        Some(ErrorCode::NotConfigured),
    );

    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(read_u8(&mut service, 1), 0);
}

#[test]
fn invalid_initial_targets_do_not_set_fault() {
    let mut output = FakeOutput::inactive();
    output.start_error = Some(OutputStartError::InvalidTargets);
    let mut service = RegisterService::new(1, 18, output, true);

    modify_control(
        &mut service,
        Opcode::Set,
        OUTPUT_ENABLE,
        Some(ErrorCode::InvalidValue),
    );

    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn technical_start_failure_reports_backend_state_and_error() {
    for state in [OutputState::Inactive, OutputState::Active] {
        let mut output = FakeOutput::inactive();
        output.start_error = Some(OutputStartError::Backend(()));
        output.state_after_error = state;
        let mut service = RegisterService::new(1, 18, output, true);

        modify_control(
            &mut service,
            Opcode::Set,
            OUTPUT_ENABLE,
            Some(ErrorCode::OutputError),
        );

        let active = if state == OutputState::Active {
            STATUS_OUTPUT_ACTIVE
        } else {
            0
        };

        assert_eq!(read_u8(&mut service, 3), 0);
        assert_eq!(
            read_u8(&mut service, 1),
            STATUS_CALIBRATION_VALID | STATUS_FAULT | active,
        );
    }
}

#[test]
fn technical_stop_failure_preserves_control_request() {
    for state in [OutputState::Inactive, OutputState::Active] {
        let mut output = FakeOutput::inactive();
        output.stop_error = true;
        output.state_after_error = state;
        let mut service = RegisterService::new(1, 18, output, true);

        modify_control(
            &mut service,
            Opcode::Set,
            OUTPUT_ENABLE,
            None,
        );
        modify_control(
            &mut service,
            Opcode::Clear,
            OUTPUT_ENABLE,
            Some(ErrorCode::OutputError),
        );

        let active = if state == OutputState::Active {
            STATUS_OUTPUT_ACTIVE
        } else {
            0
        };

        assert_eq!(read_u8(&mut service, 3), OUTPUT_ENABLE);
        assert_eq!(
            read_u8(&mut service, 1),
            STATUS_CALIBRATION_VALID | STATUS_FAULT | active,
        );
    }
}

#[test]
fn later_requests_do_not_clear_latched_fault() {
    let mut output = FakeOutput::inactive();
    output.start_error = Some(OutputStartError::Backend(()));
    let mut service = RegisterService::new(1, 18, output, true);

    modify_control(
        &mut service,
        Opcode::Set,
        OUTPUT_ENABLE,
        Some(ErrorCode::OutputError),
    );

    // A successful disable request does not clear the fault.
    modify_control(&mut service, Opcode::Write, 0, None);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID | STATUS_FAULT,
    );

    modify_control(
        &mut service,
        Opcode::Set,
        OUTPUT_ENABLE,
        Some(ErrorCode::InvalidState),
    );
}

#[test]
fn small_read_response_buffer_preserves_state_and_output() {
    let mut service = service();
    let mut output = [0xA5; 1];

    assert_eq!(
        service.handle(
            request(Opcode::Read, 0, 2, &[]),
            &mut output,
        ),
        Err(ServiceError::ResponseBufferTooSmall),
    );
    assert_eq!(output, [0xA5; 1]);
    assert_eq!(read_u8(&mut service, 3), 0);
}

#[test]
fn empty_error_response_buffer_returns_local_error() {
    let mut service = service();

    assert_eq!(
        service.handle(
            request(Opcode::Read, 0xFF, 1, &[]),
            &mut [],
        ),
        Err(ServiceError::ResponseBufferTooSmall),
    );
    assert_eq!(read_u8(&mut service, 3), 0);
}

#[test]
fn missing_nack_capacity_prevents_control_change() {
    let mut service = service();

    assert_eq!(
        service.handle(
            request(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]),
            &mut [],
        ),
        Err(ServiceError::ResponseBufferTooSmall),
    );
    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
}
