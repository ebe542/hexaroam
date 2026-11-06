extern crate std;

use self::std::{collections::VecDeque, vec::Vec};
use super::{CobsSubmitError, CobsTransmitter};
use crate::{
    ByteTransport,
    cobs_frame::{self, CobsFrameError, MAX_CONTENT_SIZE, MAX_WIRE_SIZE},
};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

fn crc32(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

fn expected_frame(content: &[u8]) -> Vec<u8> {
    let mut wire = [0; MAX_WIRE_SIZE];
    let length = cobs_frame::encode(content, &mut wire, &mut crc32).unwrap();
    wire[..length].to_vec()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Disconnected,
}

#[derive(Default)]
struct Transport {
    written: Vec<u8>,
    outcomes: VecDeque<Result<bool, TestError>>,
    calls: usize,
}

impl ByteTransport for Transport {
    type Error = TestError;

    fn try_read(&mut self) -> Result<Option<u8>, Self::Error> {
        panic!("Transmitter must not receive")
    }

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        self.calls += 1;

        let result = self
            .outcomes
            .pop_front()
            .unwrap_or(Ok(true));

        if matches!(result, Ok(true)) {
            self.written.push(byte);
        }

        result
    }
}

#[test]
fn idle_transmitter_does_not_access_transport() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();

    assert!(!sender.is_pending());
    assert_eq!(
        sender.service(&mut transport, 100),
        Ok(0)
    );
    assert_eq!(transport.calls, 0);
}

#[test]
fn sends_complete_frame_with_crc_and_delimiters() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let content = [3, 0, 0x11];
    let expected = expected_frame(&content);

    sender
        .submit(&content, &mut crc32)
        .unwrap();
    assert!(sender.is_pending());

    assert_eq!(
        sender.service(&mut transport, MAX_WIRE_SIZE),
        Ok(expected.len()),
    );

    assert_eq!(transport.written, expected);
    assert!(!sender.is_pending());
}

#[test]
fn budget_limits_transmission_and_preserves_suffix() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let expected = expected_frame(&[1]);

    sender.submit(&[1], &mut crc32).unwrap();

    assert_eq!(sender.service(&mut transport, 2), Ok(2));
    assert_eq!(transport.written, expected[..2]);
    assert!(sender.is_pending());

    assert_eq!(
        sender.service(&mut transport, MAX_WIRE_SIZE),
        Ok(expected.len() - 2),
    );

    assert_eq!(transport.written, expected);
    assert!(!sender.is_pending());
}

#[test]
fn busy_rejection_preserves_progress_without_checksum_work() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let expected = expected_frame(&[1]);

    sender.submit(&[1], &mut crc32).unwrap();
    assert_eq!(sender.service(&mut transport, 2), Ok(2));

    assert_eq!(
        sender.submit(&[], &mut |_| panic!(
            "Busy must take precedence"
        )),
        Err(CobsSubmitError::Busy),
    );

    sender
        .service(&mut transport, MAX_WIRE_SIZE)
        .unwrap();
    assert_eq!(transport.written, expected);
}

#[test]
fn backpressure_stops_service_and_retries_unaccepted_byte() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let expected = expected_frame(&[1]);

    transport
        .outcomes
        .extend([Ok(true), Ok(false)]);

    sender.submit(&[1], &mut crc32).unwrap();

    assert_eq!(
        sender.service(&mut transport, 100),
        Ok(1)
    );
    assert_eq!(transport.calls, 2);
    assert_eq!(transport.written, expected[..1]);
    assert!(sender.is_pending());

    sender
        .service(&mut transport, 100)
        .unwrap();
    assert_eq!(transport.written, expected);
}

#[test]
fn transport_error_preserves_failed_byte_without_replaying_prefix() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let expected = expected_frame(&[1]);

    transport
        .outcomes
        .extend([Ok(true), Err(TestError::Disconnected)]);

    sender.submit(&[1], &mut crc32).unwrap();

    assert_eq!(
        sender.service(&mut transport, 100),
        Err(TestError::Disconnected),
    );
    assert_eq!(transport.written, expected[..1]);
    assert!(sender.is_pending());

    sender
        .service(&mut transport, 100)
        .unwrap();
    assert_eq!(transport.written, expected);
    assert!(!sender.is_pending());
}

#[test]
fn invalid_content_leaves_sender_available() {
    let mut sender = CobsTransmitter::new();

    assert_eq!(
        sender.submit(&[], &mut crc32),
        Err(CobsSubmitError::Frame(
            CobsFrameError::EmptyContent
        )),
    );
    assert!(!sender.is_pending());

    assert_eq!(
        sender.submit(&[0; MAX_CONTENT_SIZE + 1], &mut crc32),
        Err(CobsSubmitError::Frame(
            CobsFrameError::ContentTooLong
        )),
    );
    assert!(!sender.is_pending());

    sender.submit(&[1], &mut crc32).unwrap();
    assert!(sender.is_pending());
}

#[test]
fn sender_can_be_reused_after_complete_transmission() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let mut expected = expected_frame(&[1]);
    expected.extend(expected_frame(&[2, 1]));

    sender.submit(&[1], &mut crc32).unwrap();
    sender
        .service(&mut transport, MAX_WIRE_SIZE)
        .unwrap();

    sender
        .submit(&[2, 1], &mut crc32)
        .unwrap();
    sender
        .service(&mut transport, MAX_WIRE_SIZE)
        .unwrap();

    assert_eq!(transport.written, expected);
    assert!(!sender.is_pending());
}

#[test]
fn maximum_content_can_be_queued_and_sent() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let content = [0x55; MAX_CONTENT_SIZE];
    let expected = expected_frame(&content);

    sender
        .submit(&content, &mut crc32)
        .unwrap();
    sender
        .service(&mut transport, MAX_WIRE_SIZE)
        .unwrap();

    assert_eq!(transport.written, expected);
    assert!(!sender.is_pending());
}

#[test]
fn zero_budget_does_not_access_transport_or_discard_frame() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();

    sender.submit(&[1], &mut crc32).unwrap();

    assert_eq!(sender.service(&mut transport, 0), Ok(0));
    assert_eq!(transport.calls, 0);
    assert!(sender.is_pending());
}

#[test]
fn submitted_content_is_copied_before_returning() {
    let mut sender = CobsTransmitter::new();
    let mut transport = Transport::default();
    let mut content = [3, 0, 0x11];
    let expected = expected_frame(&content);

    sender
        .submit(&content, &mut crc32)
        .unwrap();
    content.fill(0xFF);

    sender
        .service(&mut transport, MAX_WIRE_SIZE)
        .unwrap();

    assert_eq!(transport.written, expected);
}
