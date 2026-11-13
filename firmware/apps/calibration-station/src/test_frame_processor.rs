extern crate std;

use self::std::{cell::Cell, collections::VecDeque, rc::Rc, vec::Vec};
use super::*;
use crate::control::{BackendError, PULSE_READY, RUNNING};
use hexaroam_servo_protocol::cobs_frame::{self, MAX_WIRE_SIZE};
use hexaroam_servo_signal::{PulseLimits, SignalOutputState};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

fn crc32(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

fn frame(content: &[u8]) -> Vec<u8> {
    let mut wire = [0; MAX_WIRE_SIZE];
    let length = cobs_frame::encode(content, &mut wire, &mut crc32).unwrap();
    wire[..length].to_vec()
}

fn initial_status() -> Vec<u8> {
    frame(&[0x80, 0x01, 0, 0, 0, 0, 0, 0, 0, 0])
}

#[derive(Default)]
struct Observed {
    services: Cell<usize>,
    busy: Cell<bool>,
}

struct Backend {
    active: bool,
    observed: Rc<Observed>,
}

impl PulseBackend for Backend {
    fn state(&self) -> SignalOutputState {
        if self.active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        }
    }

    fn is_faulted(&mut self) -> bool {
        false
    }

    fn start(&mut self, _: u32) -> Result<(), BackendError> {
        self.active = true;
        Ok(())
    }

    fn set_pulse(&mut self, _: u32) -> Result<(), BackendError> {
        if self.observed.busy.get() {
            Err(BackendError::Busy)
        } else {
            Ok(())
        }
    }

    fn stop(&mut self) -> Result<(), BackendError> {
        self.active = false;
        Ok(())
    }

    fn service(&mut self) {
        self.observed
            .services
            .set(self.observed.services.get() + 1);
    }
}

fn setup() -> (FrameProcessor<Backend>, Rc<Observed>) {
    let observed = Rc::new(Observed::default());
    let control = CalibrationControl::new(
        Backend {
            active: false,
            observed: Rc::clone(&observed),
        },
        PulseLimits::new(1_000, 2_000).unwrap(),
    );

    (FrameProcessor::new(control), observed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportError {
    Disconnected,
}

#[derive(Default)]
struct Transport {
    input: VecDeque<u8>,
    output: Vec<u8>,
    write_results: VecDeque<Result<bool, TransportError>>,
    read_error: bool,
    read_calls: usize,
    write_calls: usize,
}

impl ByteTransport for Transport {
    type Error = TransportError;

    fn try_read(&mut self) -> Result<Option<u8>, Self::Error> {
        self.read_calls += 1;
        if core::mem::take(&mut self.read_error) {
            return Err(TransportError::Disconnected);
        }
        Ok(self.input.pop_front())
    }

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        self.write_calls += 1;
        let result = self
            .write_results
            .pop_front()
            .unwrap_or(Ok(true));

        if matches!(result, Ok(true)) {
            self.output.push(byte);
        }

        result
    }
}

fn drain(processor: &mut FrameProcessor<Backend>, transport: &mut Transport) {
    for _ in 0..512 {
        if transport.input.is_empty() && !processor.response_pending() {
            return;
        }

        processor
            .poll(transport, 256, 256, &mut crc32)
            .unwrap();
    }

    panic!("Processor did not drain within the test limit");
}

#[test]
fn zero_budgets_service_output_without_transport_access() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();
    transport.input.extend(frame(&[0x01]));

    let progress = processor
        .poll(&mut transport, 0, 0, &mut crc32)
        .unwrap();

    assert_eq!(progress, Progress::default());
    assert_eq!(observed.services.get(), 1);
    assert_eq!(transport.read_calls, 0);
    assert_eq!(transport.write_calls, 0);
}

#[test]
fn fragmented_status_request_and_response_respect_budgets() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    transport.input.extend(frame(&[0x01]));
    let mut processed = 0;

    for _ in 0..64 {
        let progress = processor
            .poll(&mut transport, 1, 2, &mut crc32)
            .unwrap();
        assert!(progress.received <= 1);
        assert!(progress.transmitted <= 2);
        processed += progress.processed;

        if transport.input.is_empty() && !processor.response_pending() {
            break;
        }
    }

    assert_eq!(processed, 1);
    assert_eq!(transport.output, initial_status());
    assert!(!processor.response_pending());
}

#[test]
fn receive_budget_limits_partial_request_consumption() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    let request = frame(&[0x01]);
    transport
        .input
        .extend(request.iter().copied());

    let progress = processor
        .poll(&mut transport, 3, 256, &mut crc32)
        .unwrap();

    assert_eq!(progress.received, 3);
    assert_eq!(progress.processed, 0);
    assert_eq!(transport.input.len(), request.len() - 3);
    assert!(transport.output.is_empty());

    drain(&mut processor, &mut transport);
    assert_eq!(transport.output, initial_status());
}

#[test]
fn consecutive_requests_preserve_order_and_status() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    transport
        .input
        .extend(frame(&[0x03, 0xDC, 0x05, 0, 0]));
    transport
        .input
        .extend(frame(&[0x02, 1]));
    transport.input.extend(frame(&[0x01]));

    drain(&mut processor, &mut transport);

    let expected = [
        frame(&[0x80, 0x03]),
        frame(&[0x80, 0x02]),
        frame(&[0x80, 0x01, 3, 0, 0, 0, 0xDC, 0x05, 0, 0]),
    ]
    .concat();

    assert_eq!(transport.output, expected);
    assert_eq!(
        processor.status(),
        PULSE_READY | RUNNING
    );
}

#[test]
fn crc_failure_is_discarded_before_following_valid_request() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    let mut damaged = [0; MAX_WIRE_SIZE];

    let length = cobs_frame::encode(
        &[0x03, 0xDC, 0x05, 0, 0],
        &mut damaged,
        &mut |bytes| crc32(bytes) ^ 1,
    )
    .unwrap();

    transport
        .input
        .extend(damaged[..length].iter().copied());
    transport.input.extend(frame(&[0x01]));

    let progress = processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();

    assert_eq!(progress.discarded, 1);
    assert_eq!(progress.processed, 1);
    assert_eq!(processor.status(), 0);

    drain(&mut processor, &mut transport);
    assert_eq!(transport.output, initial_status());
}

#[test]
fn malformed_cobs_is_discarded_and_reception_recovers() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    // Code 3 requires two data bytes, but only one precedes the delimiter.
    transport.input.extend([0, 3, 1, 0]);
    transport.input.extend(frame(&[0x01]));

    let progress = processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();

    assert_eq!(progress.discarded, 1);
    assert_eq!(progress.processed, 1);

    drain(&mut processor, &mut transport);
    assert_eq!(transport.output, initial_status());
}

#[test]
fn incoming_responses_are_ignored_without_response_loops() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    transport
        .input
        .extend(frame(&[0x80, 0x03]));
    transport
        .input
        .extend(frame(&[0x81, 0x03, 0x05]));
    transport.input.extend(frame(&[0x01]));

    let progress = processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();

    assert_eq!(progress.processed, 1);
    assert_eq!(progress.discarded, 0);

    drain(&mut processor, &mut transport);
    assert_eq!(transport.output, initial_status());
}

#[test]
fn malformed_command_returns_crc_protected_nack() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    transport.input.extend(frame(&[0x02]));

    drain(&mut processor, &mut transport);

    assert_eq!(
        transport.output,
        frame(&[0x81, 0x02, 0x02])
    );
    assert_eq!(processor.status(), 0);
}

#[test]
fn backpressure_blocks_receive_but_not_output_service() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();
    let request = frame(&[0x01]);

    transport
        .input
        .extend(request.iter().copied());
    transport
        .input
        .extend(request.iter().copied());

    let progress = processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();
    assert_eq!(progress.processed, 1);
    assert!(processor.response_pending());
    assert!(transport.output.is_empty());

    let read_calls = transport.read_calls;

    for _ in 0..3 {
        transport
            .write_results
            .push_back(Ok(false));
        let progress = processor
            .poll(&mut transport, 256, 256, &mut crc32)
            .unwrap();
        assert_eq!(progress.received, 0);
        assert_eq!(progress.transmitted, 0);
    }

    assert_eq!(transport.read_calls, read_calls);
    assert_eq!(transport.input.len(), request.len());
    assert_eq!(observed.services.get(), 4);

    drain(&mut processor, &mut transport);
    assert_eq!(
        transport.output,
        [initial_status(), initial_status()].concat(),
    );
}

#[test]
fn timeout_discards_partial_request_without_changing_control() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    let request = frame(&[0x03, 0xDC, 0x05, 0, 0]);

    transport
        .input
        .extend(request[..4].iter().copied());
    processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();

    processor.on_receive_timeout();
    transport.input.extend(frame(&[0x01]));

    drain(&mut processor, &mut transport);

    assert_eq!(processor.status(), 0);
    assert_eq!(transport.output, initial_status());
}

#[test]
fn timeout_preserves_pending_response_and_unread_complete_request() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    transport.input.extend(frame(&[0x01]));
    transport.input.extend(frame(&[0x01]));
    processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();

    assert!(processor.response_pending());
    processor.on_receive_timeout();
    assert!(processor.response_pending());

    drain(&mut processor, &mut transport);

    assert_eq!(
        transport.output,
        [initial_status(), initial_status()].concat(),
    );
}

#[test]
fn receive_error_preserves_partial_request_and_services_output() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();
    let request = frame(&[0x01]);

    transport
        .input
        .extend(request[..3].iter().copied());
    processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();

    transport.read_error = true;
    assert_eq!(
        processor.poll(&mut transport, 256, 256, &mut crc32),
        Err(ProcessingError::Receive(
            TransportError::Disconnected
        )),
    );
    assert_eq!(observed.services.get(), 2);

    transport
        .input
        .extend(request[3..].iter().copied());
    drain(&mut processor, &mut transport);

    assert_eq!(transport.output, initial_status());
}

#[test]
fn transmit_error_retains_suffix_without_repeating_accepted_bytes() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();
    transport.input.extend(frame(&[0x01]));

    processor
        .poll(&mut transport, 256, 256, &mut crc32)
        .unwrap();
    transport.write_results.extend([
        Ok(true),
        Ok(true),
        Err(TransportError::Disconnected),
    ]);

    assert_eq!(
        processor.poll(&mut transport, 256, 256, &mut crc32),
        Err(ProcessingError::Transmit(
            TransportError::Disconnected
        )),
    );
    assert_eq!(transport.output.len(), 2);
    assert!(processor.response_pending());
    assert_eq!(observed.services.get(), 2);

    drain(&mut processor, &mut transport);

    assert_eq!(transport.output, initial_status());
    assert!(!processor.response_pending());
}

#[test]
fn busy_update_returns_nack_and_preserves_last_accepted_pulse() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();

    transport
        .input
        .extend(frame(&[0x03, 0xDC, 0x05, 0, 0]));
    transport
        .input
        .extend(frame(&[0x02, 1]));
    drain(&mut processor, &mut transport);
    transport.output.clear();

    observed.busy.set(true);
    transport
        .input
        .extend(frame(&[0x03, 0xA4, 0x06, 0, 0]));
    transport.input.extend(frame(&[0x01]));

    drain(&mut processor, &mut transport);

    assert_eq!(
        transport.output,
        [
            frame(&[0x81, 0x03, 0x05]),
            frame(&[0x80, 0x01, 3, 0, 0, 0, 0xDC, 0x05, 0, 0]),
        ]
        .concat(),
    );
}
