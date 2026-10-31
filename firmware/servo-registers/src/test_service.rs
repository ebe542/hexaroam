use super::*;
use crate::test_support::FakeOutput;
use crate::{
    OUTPUT_ENABLE, OutputStartError, OutputState, STATUS_CALIBRATION_VALID,
    STATUS_FAULT, STATUS_OUTPUT_ACTIVE,
};
use hexaroam_servo_protocol::{
    Frame, Header, Opcode, ParsedFrame, validate_header,
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

fn last_error(service: &mut RegisterService<FakeOutput>) -> u32 {
    let mut output = [0; 4];
    let response = service
        .handle(
            request(Opcode::Read, 2, 4, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        response.header,
        Header {
            opcode: Opcode::Read,
            index: 2,
            length: 4,
        },
    );

    u32::from_le_bytes(response.payload.try_into().unwrap())
}

fn cause_error(service: &mut RegisterService<FakeOutput>) {
    let mut output = [];
    assert_eq!(
        service
            .handle(
                request(Opcode::Write, 0xFF, 1, &[1]),
                &mut output
            )
            .unwrap(),
        None,
    );
}

#[test]
fn version_is_returned_in_little_endian() {
    let mut service = service();
    let mut output = [0; 4];

    let response = service
        .handle(
            request(Opcode::Read, 0, 2, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        response.header,
        Header {
            opcode: Opcode::Read,
            index: 0,
            length: 2,
        },
    );
    assert_eq!(response.payload, &[0x34, 0x12]);
}

#[test]
fn status_and_servo_count_have_one_byte_responses() {
    let mut service = service();

    for (index, expected) in [(1, STATUS_CALIBRATION_VALID), (4, 18)] {
        let mut output = [0; 4];
        let response = service
            .handle(
                request(Opcode::Read, index, 1, &[]),
                &mut output,
            )
            .unwrap()
            .unwrap();

        assert_eq!(response.header.index, index);
        assert_eq!(response.header.length, 1);
        assert_eq!(response.payload, &[expected]);
    }
}

#[test]
fn last_error_starts_clear() {
    assert_eq!(
        last_error(&mut service()),
        ErrorCode::None as u32
    );
}

#[test]
fn unknown_read_returns_canonical_error_register() {
    let mut service = service();
    let mut output = [0; 4];

    let response = service
        .handle(
            request(Opcode::Read, 0xFF, 1, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(response.header.index, 2);
    assert_eq!(response.header.length, 4);
    assert_eq!(
        response.payload,
        (ErrorCode::UnknownIndex as u32).to_le_bytes(),
    );
    assert_eq!(
        last_error(&mut service),
        ErrorCode::UnknownIndex as u32,
    );
}

#[test]
fn invalid_read_lengths_return_error_register() {
    for (index, length, expected) in [
        (0, 1, ErrorCode::InvalidLength),
        (1, 2, ErrorCode::OutOfBounds),
        (2, 1, ErrorCode::InvalidLength),
    ] {
        let mut service = service();
        let mut output = [0; 4];

        let response = service
            .handle(
                request(Opcode::Read, index, length, &[]),
                &mut output,
            )
            .unwrap()
            .unwrap();

        assert_eq!(response.header.index, 2);
        assert_eq!(response.header.length, 4);
        assert_eq!(
            response.payload,
            (expected as u32).to_le_bytes()
        );
    }
}

#[test]
fn readonly_write_has_no_response_and_preserves_output() {
    let mut service = service();
    let mut output = [0xA5; 4];

    assert_eq!(
        service
            .handle(
                request(Opcode::Write, 1, 1, &[0]),
                &mut output
            )
            .unwrap(),
        None,
    );
    assert_eq!(output, [0xA5; 4]);
    assert_eq!(
        last_error(&mut service),
        ErrorCode::UnsupportedAccess as u32,
    );

    let response = service
        .handle(
            request(Opcode::Read, 1, 1, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        response.payload,
        &[STATUS_CALIBRATION_VALID]
    );
}

#[test]
fn successful_reads_do_not_clear_last_error() {
    let mut service = service();
    cause_error(&mut service);

    let mut output = [0; 4];
    assert!(
        service
            .handle(
                request(Opcode::Read, 0, 2, &[]),
                &mut output
            )
            .unwrap()
            .is_some()
    );

    assert_eq!(
        last_error(&mut service),
        ErrorCode::UnknownIndex as u32,
    );
    assert_eq!(
        last_error(&mut service),
        ErrorCode::UnknownIndex as u32,
    );
}

#[test]
fn writing_zero_explicitly_clears_last_error() {
    let mut service = service();
    cause_error(&mut service);

    let mut output = [];
    assert_eq!(
        service
            .handle(
                request(
                    Opcode::Write,
                    2,
                    4,
                    &0_u32.to_le_bytes()
                ),
                &mut output,
            )
            .unwrap(),
        None,
    );
    assert_eq!(
        last_error(&mut service),
        ErrorCode::None as u32
    );
}

#[test]
fn nonzero_write_to_last_error_is_rejected() {
    let mut service = service();
    let mut output = [];

    assert_eq!(
        service
            .handle(
                request(
                    Opcode::Write,
                    2,
                    4,
                    &1_u32.to_le_bytes()
                ),
                &mut output,
            )
            .unwrap(),
        None,
    );
    assert_eq!(
        last_error(&mut service),
        ErrorCode::InvalidValue as u32,
    );
}

#[test]
fn bit_operations_on_last_error_are_rejected() {
    for opcode in [Opcode::Set, Opcode::Clear] {
        let mut service = service();
        let mut output = [];

        assert_eq!(
            service
                .handle(
                    request(opcode, 2, 4, &0_u32.to_le_bytes()),
                    &mut output,
                )
                .unwrap(),
            None,
        );
        assert_eq!(
            last_error(&mut service),
            ErrorCode::UnsupportedAccess as u32,
        );
    }
}

#[test]
fn mismatched_write_payload_is_not_applied() {
    let mut service = service();
    let mut output = [];

    assert_eq!(
        service
            .handle(
                request(Opcode::Write, 2, 4, &[0]),
                &mut output
            )
            .unwrap(),
        None,
    );
    assert_eq!(
        last_error(&mut service),
        ErrorCode::InvalidLength as u32,
    );
}

#[test]
fn read_request_with_payload_returns_error() {
    let mut service = service();
    let mut output = [0; 4];

    let response = service
        .handle(
            request(Opcode::Read, 1, 1, &[0]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(response.header.index, 2);
    assert_eq!(
        response.payload,
        (ErrorCode::InvalidLength as u32).to_le_bytes(),
    );
}

#[test]
fn small_response_buffer_changes_neither_registers_nor_output() {
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
    assert_eq!(
        last_error(&mut service),
        ErrorCode::None as u32
    );
}

#[test]
fn small_error_response_buffer_does_not_latch_new_error() {
    let mut service = service();
    let mut output = [0xA5; 3];

    assert_eq!(
        service.handle(
            request(Opcode::Read, 0xFF, 1, &[]),
            &mut output,
        ),
        Err(ServiceError::ResponseBufferTooSmall),
    );
    assert_eq!(output, [0xA5; 3]);
    assert_eq!(
        last_error(&mut service),
        ErrorCode::None as u32
    );
}

fn read_u8(service: &mut RegisterService<FakeOutput>, index: u8) -> u8 {
    let mut output = [0; 4];
    let response = service
        .handle(
            request(Opcode::Read, index, 1, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(response.header.index, index);
    assert_eq!(response.header.length, 1);
    response.payload[0]
}

fn modify_control(
    service: &mut RegisterService<FakeOutput>,
    opcode: Opcode,
    value: u8,
) {
    let mut output = [0xA5; 4];

    assert_eq!(
        service
            .handle(
                request(opcode, 3, 1, &[value]),
                &mut output
            )
            .unwrap(),
        None,
    );
    assert_eq!(output, [0xA5; 4]);
}

#[test]
fn control_initially_reads_disabled() {
    let mut service = service();

    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn control_set_and_clear_update_control_and_status() {
    let mut service = service();

    modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);

    assert_eq!(read_u8(&mut service, 3), OUTPUT_ENABLE);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
    );

    modify_control(
        &mut service,
        Opcode::Clear,
        OUTPUT_ENABLE,
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
    );
    assert_eq!(read_u8(&mut service, 3), OUTPUT_ENABLE);

    modify_control(&mut service, Opcode::Write, 0);
    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn reserved_control_bits_latch_error_without_state_change() {
    for opcode in [Opcode::Write, Opcode::Set, Opcode::Clear] {
        let mut service = service();

        modify_control(&mut service, opcode, 0x80);

        assert_eq!(read_u8(&mut service, 3), 0);
        assert_eq!(
            read_u8(&mut service, 1),
            STATUS_CALIBRATION_VALID,
        );
        assert_eq!(
            last_error(&mut service),
            ErrorCode::InvalidValue as u32,
        );
    }
}

#[test]
fn invalid_control_width_is_rejected_before_dispatch() {
    let mut service = service();
    let mut output = [];

    assert_eq!(
        service
            .handle(
                request(Opcode::Write, 3, 2, &[1, 0]),
                &mut output,
            )
            .unwrap(),
        None,
    );
    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        last_error(&mut service),
        ErrorCode::OutOfBounds as u32,
    );
}

#[test]
fn enabling_without_calibration_sets_last_error() {
    let mut service =
        RegisterService::new(1, 18, FakeOutput::inactive(), false);

    modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);

    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(read_u8(&mut service, 1), 0);
    assert_eq!(
        last_error(&mut service),
        ErrorCode::NotConfigured as u32,
    );
}

#[test]
fn invalid_initial_targets_do_not_set_fault() {
    let mut output = FakeOutput::inactive();
    output.start_error = Some(OutputStartError::InvalidTargets);
    let mut service = RegisterService::new(1, 18, output, true);

    modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);

    assert_eq!(read_u8(&mut service, 3), 0);
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID,
    );
    assert_eq!(
        last_error(&mut service),
        ErrorCode::InvalidValue as u32,
    );
}

#[test]
fn technical_start_failure_reports_backend_state_and_error() {
    for state in [OutputState::Inactive, OutputState::Active] {
        let mut output = FakeOutput::inactive();
        output.start_error = Some(OutputStartError::Backend(()));
        output.state_after_error = state;
        let mut service = RegisterService::new(1, 18, output, true);

        modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);

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
        assert_eq!(
            last_error(&mut service),
            ErrorCode::OutputError as u32,
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

        modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);
        modify_control(
            &mut service,
            Opcode::Clear,
            OUTPUT_ENABLE,
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
        assert_eq!(
            last_error(&mut service),
            ErrorCode::OutputError as u32,
        );
    }
}

#[test]
fn clearing_last_error_does_not_clear_fault() {
    let mut output = FakeOutput::inactive();
    output.start_error = Some(OutputStartError::Backend(()));
    let mut service = RegisterService::new(1, 18, output, true);

    modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);

    let mut buffer = [];
    service
        .handle(
            request(
                Opcode::Write,
                2,
                4,
                &0_u32.to_le_bytes(),
            ),
            &mut buffer,
        )
        .unwrap();

    assert_eq!(
        last_error(&mut service),
        ErrorCode::None as u32
    );
    assert_eq!(
        read_u8(&mut service, 1),
        STATUS_CALIBRATION_VALID | STATUS_FAULT,
    );

    modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);

    assert_eq!(
        last_error(&mut service),
        ErrorCode::InvalidState as u32,
    );
}

#[test]
fn successful_control_operation_preserves_previous_last_error() {
    let mut service = service();
    cause_error(&mut service);

    modify_control(&mut service, Opcode::Set, OUTPUT_ENABLE);

    assert_eq!(read_u8(&mut service, 3), OUTPUT_ENABLE);
    assert_eq!(
        last_error(&mut service),
        ErrorCode::UnknownIndex as u32,
    );
}
