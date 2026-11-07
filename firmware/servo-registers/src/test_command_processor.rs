extern crate std;

use self::std::{cell::Cell, collections::VecDeque, rc::Rc, vec::Vec};
use super::*;
use crate::{
    OutputStartError, OutputState, TargetReadError, TargetWriteError,
    test_support::FakeOutput,
};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_protocol::{
    cobs,
    cobs_frame::{self, MAX_WIRE_SIZE},
};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

fn crc32(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

#[derive(Default)]
struct Observed {
    services: Cell<usize>,
    busy: Cell<bool>,
}

struct TestOutput {
    inner: FakeOutput,
    observed: Rc<Observed>,
}

impl OutputBackend for TestOutput {
    // true = temporary Busy, false = technical failure.
    type Error = bool;

    fn is_busy_error(error: &Self::Error) -> bool {
        *error
    }

    fn start(&mut self) -> Result<(), OutputStartError<Self::Error>> {
        self.inner
            .start()
            .map_err(|error| match error {
                OutputStartError::InvalidTargets => {
                    OutputStartError::InvalidTargets
                }
                OutputStartError::Backend(()) => {
                    OutputStartError::Backend(false)
                }
            })
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        self.inner.stop().map_err(|()| false)
    }

    fn state(&self) -> OutputState {
        self.inner.state()
    }

    fn target(&self, id: ServoId) -> Result<Angle, TargetReadError> {
        self.inner.target(id)
    }

    fn set_targets(
        &mut self,
        targets: &[ServoTarget],
    ) -> Result<(), TargetWriteError<Self::Error>> {
        if self.observed.busy.get() {
            return Err(TargetWriteError::Backend(true));
        }

        self.inner
            .set_targets(targets)
            .map_err(|error| match error {
                TargetWriteError::UnknownServo(id) => {
                    TargetWriteError::UnknownServo(id)
                }
                TargetWriteError::DuplicateServo(id) => {
                    TargetWriteError::DuplicateServo(id)
                }
                TargetWriteError::InvalidAngle(id) => {
                    TargetWriteError::InvalidAngle(id)
                }
                TargetWriteError::UninitializedServo(id) => {
                    TargetWriteError::UninitializedServo(id)
                }
                TargetWriteError::Backend(()) => {
                    TargetWriteError::Backend(false)
                }
            })
    }

    fn service(&mut self) {
        self.observed
            .services
            .set(self.observed.services.get() + 1);
        self.inner.service();
    }
}

type Processor = CommandProcessor<TestOutput, 2>;

fn setup() -> (Processor, Rc<Observed>) {
    let observed = Rc::new(Observed::default());
    let service = CommandService::new(
        TestOutput {
            inner: FakeOutput::inactive(),
            observed: Rc::clone(&observed),
        },
        true,
    );

    (CommandProcessor::new(service), observed)
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

        if self.read_error {
            self.read_error = false;
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

fn wire(content: &[u8]) -> Vec<u8> {
    let mut output = [0; MAX_WIRE_SIZE];
    let length = cobs_frame::encode(content, &mut output, &mut crc32).unwrap();
    output[..length].to_vec()
}

fn enqueue(transport: &mut Transport, content: &[u8]) {
    transport.input.extend(wire(content));
}

fn drain(processor: &mut Processor, transport: &mut Transport) {
    for _ in 0..100 {
        processor
            .poll(transport, 512, 512, &mut crc32)
            .unwrap();

        if transport.input.is_empty() && !processor.response_pending() {
            return;
        }
    }

    panic!("Processor did not finish within the test limit");
}

fn targets(first: u8, angles: &[i32]) -> Vec<u8> {
    let mut content = Vec::from([0x03, first]);
    for angle in angles {
        content.extend(angle.to_le_bytes());
    }
    content
}

#[test]
fn fragmented_status_obeys_budgets_and_defers_new_response() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    enqueue(&mut transport, &[0x01]);

    let progress = processor
        .poll(&mut transport, 2, 512, &mut crc32)
        .unwrap();

    assert_eq!(progress.received, 2);
    assert_eq!(progress.processed, 0);
    assert!(!progress.receive_empty);
    assert!(transport.output.is_empty());

    let progress = processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();

    assert_eq!(progress.processed, 1);
    assert_eq!(progress.transmitted, 0);
    assert!(processor.response_pending());
    assert!(transport.output.is_empty());

    let progress = processor
        .poll(&mut transport, 0, 2, &mut crc32)
        .unwrap();

    assert_eq!(progress.transmitted, 2);
    assert!(processor.response_pending());

    drain(&mut processor, &mut transport);
    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn enable_initial_targets_and_status_execute_in_order() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    enqueue(&mut transport, &[0x02, 1]);
    enqueue(&mut transport, &targets(0, &[100, 200]));
    enqueue(&mut transport, &[0x01]);

    drain(&mut processor, &mut transport);

    let mut expected = wire(&[0x80, 0x02]);
    expected.extend(wire(&[0x80, 0x03]));
    expected.extend(wire(&[0x80, 0x01, 7, 0, 0, 0]));

    assert_eq!(transport.output, expected);
}

#[test]
fn unknown_command_and_invalid_parameters_receive_nack() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    enqueue(&mut transport, &[0xFE]);
    enqueue(&mut transport, &[0x01, 0]);
    enqueue(&mut transport, &[0x02, 2]);

    drain(&mut processor, &mut transport);

    let mut expected = wire(&[0x81, 0xFE, 1]);
    expected.extend(wire(&[0x81, 0x01, 2]));
    expected.extend(wire(&[0x81, 0x02, 3]));

    assert_eq!(transport.output, expected);
}

#[test]
fn incoming_replies_are_ignored_without_response_loop() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    enqueue(&mut transport, &[0x80, 0x02]);
    enqueue(&mut transport, &[0x81, 0x03, 5]);
    enqueue(&mut transport, &[0x01]);

    drain(&mut processor, &mut transport);

    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn crc_failure_does_not_enable_output_and_next_frame_recovers() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    let mut decoded = Vec::from([0x02, 1]);
    decoded.extend((crc32(&[0x02, 1]) ^ 1).to_le_bytes());

    let mut encoded = [0; 256];
    let length = cobs::encode(&decoded, &mut encoded).unwrap();

    transport.input.push_back(0);
    transport
        .input
        .extend(&encoded[..length]);
    transport.input.push_back(0);
    enqueue(&mut transport, &[0x01]);

    drain(&mut processor, &mut transport);

    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn malformed_cobs_is_discarded_and_following_status_succeeds() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    transport.input.extend([0, 3, 0x11, 0]);
    enqueue(&mut transport, &[0x01]);

    let progress = processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();

    assert_eq!(progress.discarded, 1);
    assert_eq!(progress.processed, 1);

    drain(&mut processor, &mut transport);
    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn overflow_discards_suffix_until_delimiter_then_recovers() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    transport.input.push_back(0);
    transport.input.extend([1; 257]);
    transport.input.extend([2, 0x11, 0]);
    enqueue(&mut transport, &[0x01]);

    let progress = processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();

    assert_eq!(progress.discarded, 1);

    drain(&mut processor, &mut transport);
    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn backpressure_blocks_reception_but_not_output_servicing() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();

    enqueue(&mut transport, &[0x01]);
    enqueue(&mut transport, &[0x02, 1]);

    processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();
    let unread = transport.input.len();
    let reads = transport.read_calls;

    transport
        .write_results
        .push_back(Ok(false));

    let progress = processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();

    assert_eq!(progress.transmitted, 0);
    assert_eq!(progress.received, 0);
    assert_eq!(transport.input.len(), unread);
    assert_eq!(transport.read_calls, reads);
    assert_eq!(observed.services.get(), 2);
    assert!(processor.response_pending());

    drain(&mut processor, &mut transport);

    let mut expected = wire(&[0x80, 0x01, 1, 0, 0, 0]);
    expected.extend(wire(&[0x80, 0x02]));
    assert_eq!(transport.output, expected);
}

#[test]
fn transmit_error_preserves_suffix_without_replaying_prefix() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    enqueue(&mut transport, &[0x01]);

    processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();

    transport
        .write_results
        .extend([Ok(true), Err(TransportError::Disconnected)]);

    assert_eq!(
        processor.poll(&mut transport, 512, 512, &mut crc32),
        Err(CommandProcessingError::Transmit(
            TransportError::Disconnected,
        )),
    );
    assert_eq!(transport.output.len(), 1);
    assert!(processor.response_pending());

    drain(&mut processor, &mut transport);
    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn receive_error_preserves_partial_frame_until_caller_resets_it() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();
    enqueue(&mut transport, &[0x01]);

    processor
        .poll(&mut transport, 3, 512, &mut crc32)
        .unwrap();
    transport.read_error = true;

    assert_eq!(
        processor.poll(&mut transport, 512, 512, &mut crc32),
        Err(CommandProcessingError::Receive(
            TransportError::Disconnected,
        )),
    );

    drain(&mut processor, &mut transport);
    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn timeout_discards_partial_request_and_accepts_new_frame() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    transport.input.extend([0, 6, 2]);
    processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();

    processor.on_receive_timeout();

    enqueue(&mut transport, &[0x01]);
    drain(&mut processor, &mut transport);

    assert_eq!(
        transport.output,
        wire(&[0x80, 0x01, 1, 0, 0, 0])
    );
}

#[test]
fn timeout_preserves_pending_reply_and_unread_requests() {
    let (mut processor, _) = setup();
    let mut transport = Transport::default();

    enqueue(&mut transport, &[0x01]);
    enqueue(&mut transport, &[0x02, 1]);

    processor
        .poll(&mut transport, 512, 512, &mut crc32)
        .unwrap();
    assert!(processor.response_pending());

    processor.on_receive_timeout();
    drain(&mut processor, &mut transport);

    let mut expected = wire(&[0x80, 0x01, 1, 0, 0, 0]);
    expected.extend(wire(&[0x80, 0x02]));
    assert_eq!(transport.output, expected);
}

#[test]
fn zero_budgets_still_service_output_without_transport_access() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();
    enqueue(&mut transport, &[0x01]);

    let progress = processor
        .poll(&mut transport, 0, 0, &mut crc32)
        .unwrap();

    assert_eq!(progress, CommandProgress::default());
    assert_eq!(observed.services.get(), 1);
    assert_eq!(transport.read_calls, 0);
    assert_eq!(transport.write_calls, 0);
}

#[test]
fn backend_busy_returns_nack_without_latching_output_error() {
    let (mut processor, observed) = setup();
    let mut transport = Transport::default();
    observed.busy.set(true);

    enqueue(&mut transport, &[0x02, 1]);
    enqueue(&mut transport, &targets(0, &[100, 200]));
    enqueue(&mut transport, &[0x01]);

    drain(&mut processor, &mut transport);

    let mut expected = wire(&[0x80, 0x02]);
    expected.extend(wire(&[0x81, 0x03, 5]));
    expected.extend(wire(&[0x80, 0x01, 3, 0, 0, 0]));

    assert_eq!(transport.output, expected);
}
