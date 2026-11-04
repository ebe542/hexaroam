use super::*;
use hexaroam_servo_control::Calibration;

fn calibration() -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .unwrap()
}

fn queue_calibration(processor: &mut Processor, transport: &mut TestTransport) {
    let mut payload = [0; MAX_PAYLOAD_SIZE];
    let length = hexaroam_servo_storage::encode(
        &[Some(calibration()); 18],
        &mut payload,
        &mut SoftwareCrc32,
    )
    .unwrap();

    let bytes = request(
        Opcode::Write,
        0x10,
        length as u16,
        &payload[..length],
    );
    transport.feed(&bytes);

    let progress = poll(processor, transport, MAX_FRAME_SIZE, 0);

    assert_eq!(progress.received, bytes.len());
    assert_eq!(progress.processed, 1);
    assert_eq!(progress.transmitted, 0);
    assert!(processor.response_pending());
    assert!(transport.outgoing.is_empty());
}

fn assert_ready(processor: &Processor) {
    let pending = processor
        .calibration_ready()
        .expect("Request must be available after response transmission");

    assert_eq!(
        pending.calibrations(),
        &[Some(calibration()); 18],
    );
}

#[test]
fn no_request_is_ready_initially() {
    let (processor, _) = processor();

    assert!(!processor.response_pending());
    assert!(processor.calibration_ready().is_none());
}

#[test]
fn queued_ack_prevents_release() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    queue_calibration(&mut processor, &mut transport);

    assert!(processor.calibration_ready().is_none());

    poll(&mut processor, &mut transport, 0, 0);

    assert!(processor.calibration_ready().is_none());
    assert!(transport.outgoing.is_empty());
}

#[test]
fn partial_ack_prevents_release_until_final_byte_is_accepted() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    queue_calibration(&mut processor, &mut transport);

    let progress = poll(&mut processor, &mut transport, 0, 7);

    assert_eq!(progress.transmitted, 7);
    assert_eq!(transport.outgoing.len(), 7);
    assert!(processor.calibration_ready().is_none());

    let progress = poll(&mut processor, &mut transport, 0, 1);

    assert_eq!(progress.transmitted, 1);
    assert!(!processor.response_pending());
    assert_response(&transport.outgoing, 0x10, &[]);
    assert_ready(&processor);

    // Merely borrowing the request does not consume it.
    assert_ready(&processor);
}

#[test]
fn backpressure_preserves_unreleased_request() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();
    queue_calibration(&mut processor, &mut transport);

    transport
        .write_results
        .push_back(Ok(false));

    let progress = poll(&mut processor, &mut transport, 0, 100);

    assert_eq!(progress.transmitted, 0);
    assert!(processor.calibration_ready().is_none());
    assert_eq!(trace.services.get(), 2);

    poll(&mut processor, &mut transport, 0, 100);

    assert_response(&transport.outgoing, 0x10, &[]);
    assert_ready(&processor);
    assert_eq!(trace.starts.get(), 0);
    assert_eq!(trace.writes.get(), 0);
}

#[test]
fn transmit_error_does_not_release_or_duplicate_ack() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    queue_calibration(&mut processor, &mut transport);

    transport
        .write_results
        .push_back(Ok(true));
    transport
        .write_results
        .push_back(Err(IoError::Write));

    assert_eq!(
        processor.poll(&mut transport, 0, 100, &mut checksum),
        Err(FrameProcessingError::Transmit(
            IoError::Write
        )),
    );

    assert_eq!(transport.outgoing.len(), 1);
    assert!(processor.calibration_ready().is_none());

    let progress = poll(&mut processor, &mut transport, 0, 100);

    assert_eq!(progress.transmitted, 7);
    assert_eq!(transport.outgoing.len(), 8);
    assert_response(&transport.outgoing, 0x10, &[]);
    assert_ready(&processor);
}

#[test]
fn pending_status_response_also_delays_release() {
    let (mut processor, _) = processor();
    let mut transport = TestTransport::default();
    queue_calibration(&mut processor, &mut transport);
    poll(&mut processor, &mut transport, 0, 100);

    assert_response(&transport.outgoing, 0x10, &[]);
    assert_ready(&processor);
    transport.outgoing.clear();

    queue_read(&mut processor, &mut transport, 1, 4);

    assert!(processor.calibration_ready().is_none());

    poll(&mut processor, &mut transport, 0, 100);

    assert_response(
        &transport.outgoing,
        1,
        &(STATUS_CALIBRATION_VALID | (1_u32 << 2)).to_le_bytes(),
    );
    assert_ready(&processor);
}

#[test]
fn completion_is_forwarded_and_visible_in_status() {
    for result in [
        CalibrationSaveResult::Success,
        CalibrationSaveResult::StorageError,
        CalibrationSaveResult::VerificationFailed,
    ] {
        let (mut processor, _) = processor();
        let mut transport = TestTransport::default();

        queue_calibration(&mut processor, &mut transport);
        poll(&mut processor, &mut transport, 0, 100);
        assert_ready(&processor);

        assert_eq!(
            processor.complete_calibration(result),
            Ok(())
        );
        assert!(processor.calibration_ready().is_none());

        transport.outgoing.clear();
        queue_read(&mut processor, &mut transport, 1, 4);
        poll(&mut processor, &mut transport, 0, 100);

        let expected = STATUS_CALIBRATION_VALID | ((result as u32) << 8);

        assert_response(
            &transport.outgoing,
            1,
            &expected.to_le_bytes(),
        );
    }
}

#[test]
fn completion_without_request_preserves_service_error() {
    let (mut processor, _) = processor();

    assert_eq!(
        processor.complete_calibration(CalibrationSaveResult::Success),
        Err(CalibrationCompletionError::NoPendingCalibration),
    );
}

#[path = "test_calibration_persistence.rs"]
mod persistence;
