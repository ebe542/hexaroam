//! Target-register behavior with a simulated output backend.

use crate::{
    ErrorCode, OUTPUT_ENABLE, REGISTER_REGIONS, RegisterService,
    STATUS_CALIBRATION_VALID, STATUS_FAULT, STATUS_OUTPUT_ACTIVE, ServiceError,
    test_support::FakeOutput,
};
use hexaroam_servo_control::Angle;
use hexaroam_servo_protocol::{
    Frame, Header, Opcode, ParsedFrame, validate_header,
};

fn service() -> RegisterService<FakeOutput> {
    RegisterService::new(1, 18, FakeOutput::inactive(), true)
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
    service: &mut RegisterService<FakeOutput>,
    opcode: Opcode,
    index: u8,
    length: u16,
    payload: &[u8],
) -> Result<(), u8> {
    let mut output = [0xA5; 72];
    let response = service
        .handle(
            request(opcode, index, length, payload),
            &mut output,
        )
        .unwrap()
        .expect("Every request must receive a response");

    assert_eq!(response.header.index, index);

    match response.header.opcode {
        Opcode::Ack => {
            assert_eq!(response.header.length, 0);
            assert!(response.payload.is_empty());
            assert_eq!(output, [0xA5; 72]);
            Ok(())
        }
        Opcode::Nack => {
            assert_eq!(response.header.length, 1);
            assert_eq!(response.payload.len(), 1);
            let error = response.payload[0];
            assert_eq!(&output[1..], &[0xA5; 71]);
            Err(error)
        }
        other => panic!("Unexpected response opcode: {other:?}"),
    }
}

fn write_targets(
    service: &mut RegisterService<FakeOutput>,
    first: u8,
    values: &[i32],
) -> Result<(), u8> {
    let mut payload = [0; 72];

    for (index, value) in values.iter().enumerate() {
        payload[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }

    let length = values.len() * 4;

    modify(
        service,
        Opcode::Write,
        0x20 + first,
        length as u16,
        &payload[..length],
    )
}

fn read_targets<const N: usize>(
    service: &mut RegisterService<FakeOutput>,
    first: u8,
) -> [i32; N] {
    let mut output = [0; 72];
    let response = service
        .handle(
            request(
                Opcode::Read,
                0x20 + first,
                (N * 4) as u16,
                &[],
            ),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        response.header,
        Header {
            opcode: Opcode::Ack,
            index: 0x20 + first,
            length: (N * 4) as u16,
        },
    );
    assert_eq!(response.payload.len(), N * 4);

    let mut values = [0; N];

    for (index, value) in values.iter_mut().enumerate() {
        *value = i32::from_le_bytes(
            response.payload[index * 4..index * 4 + 4]
                .try_into()
                .unwrap(),
        );
    }

    values
}

fn scalar(
    service: &mut RegisterService<FakeOutput>,
    index: u8,
    width: u16,
) -> u32 {
    let mut output = [0; 4];
    let response = service
        .handle(
            request(Opcode::Read, index, width, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(response.header.opcode, Opcode::Ack);
    assert_eq!(response.header.index, index);
    assert_eq!(response.header.length, width);

    let mut bytes = [0; 4];
    bytes[..usize::from(width)].copy_from_slice(response.payload);
    u32::from_le_bytes(bytes)
}

#[test]
fn initial_targets_are_supplied_by_backend() {
    let mut output = FakeOutput::inactive();
    output.targets[0] = Angle::from_centidegrees(-1_234);
    output.targets[17] = Angle::from_centidegrees(5_678);

    let mut service = RegisterService::new(1, 18, output, true);
    let targets = read_targets::<18>(&mut service, 0);

    assert_eq!(targets[0], -1_234);
    assert_eq!(targets[17], 5_678);
    assert_eq!(&targets[1..17], &[0; 16]);
}

#[test]
fn inactive_writes_prepare_single_and_consecutive_targets() {
    let mut service = service();

    write_targets(&mut service, 17, &[-4_500]).unwrap();
    write_targets(&mut service, 3, &[-9_000, 1_234, 9_000]).unwrap();

    let targets = read_targets::<18>(&mut service, 0);
    let mut expected = [0; 18];
    expected[3] = -9_000;
    expected[4] = 1_234;
    expected[5] = 9_000;
    expected[17] = -4_500;

    assert_eq!(targets, expected);
    assert_eq!(
        scalar(&mut service, 1, 4),
        STATUS_CALIBRATION_VALID,
    );
}

#[test]
fn active_write_updates_targets_without_disabling_output() {
    let mut service = service();

    modify(
        &mut service,
        Opcode::Set,
        3,
        1,
        &[OUTPUT_ENABLE],
    )
    .unwrap();
    write_targets(&mut service, 0, &[1_234, -5_678]).unwrap();

    assert_eq!(
        read_targets::<2>(&mut service, 0),
        [1_234, -5_678]
    );
    assert_eq!(
        scalar(&mut service, 1, 4),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE,
    );
}

#[test]
fn invalid_angle_rejects_complete_batch() {
    for active in [false, true] {
        for invalid in [-9_001, 9_001, i32::MIN, i32::MAX] {
            let mut service = service();
            write_targets(&mut service, 0, &[100, 200]).unwrap();

            if active {
                modify(
                    &mut service,
                    Opcode::Set,
                    3,
                    1,
                    &[OUTPUT_ENABLE],
                )
                .unwrap();
            }

            assert_eq!(
                write_targets(&mut service, 0, &[300, invalid]),
                Err(ErrorCode::InvalidValue as u8),
            );

            assert_eq!(
                read_targets::<2>(&mut service, 0),
                [100, 200]
            );

            let expected_status = STATUS_CALIBRATION_VALID
                | if active { STATUS_OUTPUT_ACTIVE } else { 0 };

            assert_eq!(
                scalar(&mut service, 1, 4),
                expected_status,
            );
        }
    }
}

#[test]
fn invalid_lengths_access_and_boundaries_preserve_targets() {
    let cases: &[(Opcode, u8, u16, &[u8], ErrorCode)] = &[
        (
            Opcode::Write,
            0x20,
            0,
            &[],
            ErrorCode::InvalidLength,
        ),
        (
            Opcode::Write,
            0x20,
            3,
            &[0; 3],
            ErrorCode::InvalidLength,
        ),
        (
            Opcode::Write,
            0x20,
            4,
            &[0; 3],
            ErrorCode::InvalidLength,
        ),
        (
            Opcode::Write,
            0x31,
            8,
            &[0; 8],
            ErrorCode::OutOfBounds,
        ),
        (
            Opcode::Set,
            0x20,
            4,
            &[0; 4],
            ErrorCode::UnsupportedAccess,
        ),
        (
            Opcode::Clear,
            0x20,
            4,
            &[0; 4],
            ErrorCode::UnsupportedAccess,
        ),
    ];

    for &(opcode, index, length, payload, expected) in cases {
        let mut service = service();
        write_targets(&mut service, 0, &[100]).unwrap();

        assert_eq!(
            modify(
                &mut service,
                opcode,
                index,
                length,
                payload
            ),
            Err(expected as u8),
        );

        assert_eq!(
            read_targets::<1>(&mut service, 0),
            [100]
        );
    }
}

#[test]
fn missing_calibration_rejects_target_reads_and_writes() {
    let mut service =
        RegisterService::new(1, 18, FakeOutput::inactive(), false);

    assert_eq!(
        write_targets(&mut service, 0, &[100]),
        Err(ErrorCode::NotConfigured as u8),
    );

    let mut output = [0xA5; 4];
    let response = service
        .handle(
            request(Opcode::Read, 0x20, 4, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        response.header,
        Header {
            opcode: Opcode::Nack,
            index: 0x20,
            length: 1,
        },
    );
    assert_eq!(
        response.payload,
        &[ErrorCode::NotConfigured as u8]
    );
    assert_eq!(&output[1..], &[0xA5; 3]);
}

#[test]
fn backend_failure_latches_fault_and_blocks_further_target_writes() {
    let mut output = FakeOutput::inactive();
    output.target_error = true;

    let mut service = RegisterService::new(1, 18, output, true);

    modify(
        &mut service,
        Opcode::Set,
        3,
        1,
        &[OUTPUT_ENABLE],
    )
    .unwrap();
    assert_eq!(
        write_targets(&mut service, 0, &[100, 200]),
        Err(ErrorCode::OutputError as u8),
    );

    assert_eq!(
        read_targets::<2>(&mut service, 0),
        [0, 0]
    );
    assert_eq!(
        scalar(&mut service, 1, 4),
        STATUS_CALIBRATION_VALID | STATUS_OUTPUT_ACTIVE | STATUS_FAULT,
    );

    assert_eq!(
        write_targets(&mut service, 0, &[300]),
        Err(ErrorCode::InvalidState as u8),
    );

    assert_eq!(read_targets::<1>(&mut service, 0), [0]);
}

#[test]
fn small_read_buffer_preserves_targets_and_output() {
    let mut service = service();
    write_targets(&mut service, 0, &[100, 200]).unwrap();
    let mut output = [0xA5; 7];

    assert_eq!(
        service.handle(
            request(Opcode::Read, 0x20, 8, &[]),
            &mut output,
        ),
        Err(ServiceError::ResponseBufferTooSmall),
    );
    assert_eq!(output, [0xA5; 7]);
    assert_eq!(
        read_targets::<2>(&mut service, 0),
        [100, 200]
    );
}
