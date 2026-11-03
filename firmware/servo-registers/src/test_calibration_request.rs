extern crate std;

use self::std::vec::Vec;

use crate::{
    CalibrationCompletionError, CalibrationSaveResult, ErrorCode,
    OUTPUT_ENABLE, OutputState, REGISTER_REGIONS, RegisterService,
    test_support::FakeOutput,
};
use hexaroam_servo_control::{Angle, Calibration};
use hexaroam_servo_protocol::{
    Frame, Header, Opcode, ParsedFrame, validate_header,
};
use hexaroam_servo_storage::{SoftwareCrc32, encode};

const CALIBRATION_INDEX: u8 = 0x10;
const BUSY: u32 = 1 << 2;
const RESULT_SHIFT: u32 = 8;

fn service() -> RegisterService<FakeOutput> {
    // No active calibration: completeness still requires eighteen entries.
    RegisterService::new(2, 0, FakeOutput::inactive(), false)
}

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .unwrap()
}

fn dataset(complete: bool) -> Vec<u8> {
    let mut entries = [Some(calibration()); 18];

    if !complete {
        entries[17] = None;
    }

    let mut bytes = [0; 512];
    let length = encode(&entries, &mut bytes, &mut SoftwareCrc32).unwrap();

    bytes[..length].to_vec()
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
    payload: &[u8],
) {
    let index = request.frame.header.index;
    let mut output = [0xA5; 512];

    let response = service
        .handle(request, &mut output)
        .unwrap()
        .expect("Every handled request must produce a response");

    assert_eq!(
        response.header,
        Header {
            opcode,
            index,
            length: payload.len() as u16,
        },
    );
    assert_eq!(response.payload, payload);
    assert!(
        output[payload.len()..]
            .iter()
            .all(|&byte| byte == 0xA5)
    );
}

fn submit(service: &mut RegisterService<FakeOutput>, bytes: &[u8]) {
    assert_reply(
        service,
        request(
            Opcode::Write,
            CALIBRATION_INDEX,
            bytes.len() as u16,
            bytes,
        ),
        Opcode::Ack,
        &[],
    );
}

fn read(
    service: &mut RegisterService<FakeOutput>,
    index: u8,
    width: u16,
) -> Vec<u8> {
    let mut output = [0; 72];

    let response = service
        .handle(
            request(Opcode::Read, index, width, &[]),
            &mut output,
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        response.header,
        Header {
            opcode: Opcode::Ack,
            index,
            length: width,
        },
    );
    assert_eq!(
        response.payload.len(),
        usize::from(width)
    );

    response.payload.to_vec()
}

fn status(service: &mut RegisterService<FakeOutput>) -> u32 {
    u32::from_le_bytes(read(service, 1, 4).try_into().unwrap())
}

#[test]
fn initial_state_has_no_request_busy_or_result() {
    let mut service = service();

    assert!(service.pending_calibration().is_none());
    assert_eq!(status(&mut service), 0);
}

#[test]
fn complete_write_is_accepted_without_active_calibration() {
    let mut service = service();
    submit(&mut service, &dataset(true));

    assert_eq!(status(&mut service), BUSY);
    assert_eq!(read(&mut service, 4, 1), &[0]);

    assert_eq!(
        service
            .pending_calibration()
            .unwrap()
            .calibrations(),
        &[Some(calibration()); 18],
    );
}

#[test]
fn accepted_data_is_independent_of_receive_buffer() {
    let mut service = service();

    {
        let mut bytes = dataset(true);
        submit(&mut service, &bytes);
        bytes.fill(0);
    }

    assert_eq!(
        service
            .pending_calibration()
            .unwrap()
            .calibrations(),
        &[Some(calibration()); 18],
    );
    assert_eq!(status(&mut service), BUSY);
}

#[test]
fn active_output_rejects_calibration_without_changing_state() {
    let mut output = FakeOutput::inactive();
    output.state = OutputState::Active;

    let mut service = RegisterService::new(2, 18, output, true);
    let before = status(&mut service);
    let bytes = dataset(true);

    assert_reply(
        &mut service,
        request(
            Opcode::Write,
            CALIBRATION_INDEX,
            bytes.len() as u16,
            &bytes,
        ),
        Opcode::Nack,
        &[ErrorCode::InvalidState as u8],
    );

    assert!(service.pending_calibration().is_none());
    assert_eq!(status(&mut service), before);
    assert_eq!(
        read(&mut service, 3, 1),
        &[OUTPUT_ENABLE]
    );
}

#[test]
fn busy_rejects_all_modifications_without_replacing_request() {
    let mut service = service();
    let bytes = dataset(true);
    submit(&mut service, &bytes);

    for (opcode, index, payload) in [
        (
            Opcode::Write,
            CALIBRATION_INDEX,
            bytes.as_slice(),
        ),
        (Opcode::Write, 3, &[0][..]),
        (Opcode::Set, 3, &[OUTPUT_ENABLE][..]),
        (Opcode::Clear, 3, &[OUTPUT_ENABLE][..]),
        (Opcode::Write, 0x20, &[0; 4][..]),
    ] {
        assert_reply(
            &mut service,
            request(
                opcode,
                index,
                payload.len() as u16,
                payload,
            ),
            Opcode::Nack,
            &[ErrorCode::InvalidState as u8],
        );

        assert_eq!(status(&mut service), BUSY);
        assert_eq!(
            service
                .pending_calibration()
                .unwrap()
                .calibrations(),
            &[Some(calibration()); 18],
        );
    }
}

#[test]
fn ordinary_reads_remain_available_while_busy() {
    let mut service = RegisterService::new(2, 18, FakeOutput::inactive(), true);

    submit(&mut service, &dataset(true));

    assert_eq!(read(&mut service, 0, 2), &[2, 0]);
    assert_eq!(read(&mut service, 3, 1), &[0]);
    assert_eq!(read(&mut service, 4, 1), &[18]);
    assert_eq!(read(&mut service, 0x20, 4), &[0; 4]);

    assert_ne!(status(&mut service) & BUSY, 0);
    assert!(service.pending_calibration().is_some());
}

#[test]
fn invalid_data_does_not_create_request_or_set_busy() {
    let mut damaged = dataset(true);
    let last = damaged.len() - 1;
    damaged[last] ^= 1;

    for (bytes, error) in [
        (dataset(false), ErrorCode::InvalidLength),
        (damaged, ErrorCode::InvalidValue),
    ] {
        let mut service = service();

        assert_reply(
            &mut service,
            request(
                Opcode::Write,
                CALIBRATION_INDEX,
                bytes.len() as u16,
                &bytes,
            ),
            Opcode::Nack,
            &[error as u8],
        );

        assert!(service.pending_calibration().is_none());
        assert_eq!(status(&mut service), 0);
    }
}

#[test]
fn missing_response_capacity_prevents_acceptance() {
    let mut service = service();
    let bytes = dataset(true);

    assert_eq!(
        service.handle(
            request(
                Opcode::Write,
                CALIBRATION_INDEX,
                bytes.len() as u16,
                &bytes,
            ),
            &mut [],
        ),
        Err(crate::ServiceError::ResponseBufferTooSmall),
    );

    assert!(service.pending_calibration().is_none());
    assert_eq!(status(&mut service), 0);
}

#[test]
fn completion_records_result_and_releases_request() {
    for result in [
        CalibrationSaveResult::Success,
        CalibrationSaveResult::StorageError,
        CalibrationSaveResult::VerificationFailed,
    ] {
        let mut service = service();
        submit(&mut service, &dataset(true));

        assert_eq!(
            service.complete_calibration(result),
            Ok(()),
        );

        assert!(service.pending_calibration().is_none());
        assert_eq!(
            status(&mut service),
            (result as u32) << RESULT_SHIFT,
        );

        // Completion does not activate calibration or change ServoCount.
        assert_eq!(read(&mut service, 4, 1), &[0]);
    }
}

#[test]
fn completion_without_request_is_rejected() {
    let mut service = service();

    assert_eq!(
        service.complete_calibration(CalibrationSaveResult::Success),
        Err(CalibrationCompletionError::NoPendingCalibration),
    );
    assert_eq!(status(&mut service), 0);
}

#[test]
fn rejected_requests_preserve_previous_operation_result() {
    let mut service = service();
    submit(&mut service, &dataset(true));
    service
        .complete_calibration(CalibrationSaveResult::StorageError)
        .unwrap();

    assert_reply(
        &mut service,
        request(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]),
        Opcode::Nack,
        &[ErrorCode::NotConfigured as u8],
    );

    assert_eq!(
        status(&mut service),
        (CalibrationSaveResult::StorageError as u32) << RESULT_SHIFT,
    );
}

#[test]
fn next_accepted_request_resets_previous_result() {
    let mut service = service();
    let bytes = dataset(true);

    submit(&mut service, &bytes);
    service
        .complete_calibration(CalibrationSaveResult::StorageError)
        .unwrap();

    submit(&mut service, &bytes);

    assert_eq!(status(&mut service), BUSY);
    assert!(service.pending_calibration().is_some());
}
