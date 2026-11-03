//! Complete request processing with a simulated byte transport.

extern crate std;

use self::std::{cell::Cell, collections::VecDeque, rc::Rc, vec::Vec};

use super::*;
use crate::{
    OUTPUT_ENABLE, OutputStartError, OutputState, STATUS_CALIBRATION_VALID,
    TargetReadError, TargetWriteError, test_support::FakeOutput,
};
use hexaroam_servo_control::{Angle, ServoId, ServoTarget};
use hexaroam_servo_protocol::{Header, Opcode, decode_frame, encode_frame};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IoError {
    Read,
    Write,
}

#[derive(Default)]
struct TestTransport {
    incoming: VecDeque<Result<u8, IoError>>,
    outgoing: Vec<u8>,
    write_results: VecDeque<Result<bool, IoError>>,
    read_calls: usize,
    write_calls: usize,
}

impl TestTransport {
    fn feed(&mut self, bytes: &[u8]) {
        self.incoming
            .extend(bytes.iter().copied().map(Ok));
    }
}

impl ByteTransport for TestTransport {
    type Error = IoError;

    fn try_read(&mut self) -> Result<Option<u8>, Self::Error> {
        self.read_calls += 1;

        match self.incoming.pop_front() {
            Some(Ok(byte)) => Ok(Some(byte)),
            Some(Err(error)) => Err(error),
            None => Ok(None),
        }
    }

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        self.write_calls += 1;
        let result = self
            .write_results
            .pop_front()
            .unwrap_or(Ok(true));

        if matches!(result, Ok(true)) {
            self.outgoing.push(byte);
        }

        result
    }
}

#[derive(Default)]
struct Trace {
    services: Cell<usize>,
    starts: Cell<usize>,
    writes: Cell<usize>,
}

struct ObservedOutput {
    inner: FakeOutput,
    trace: Rc<Trace>,
}

impl OutputBackend for ObservedOutput {
    type Error = ();

    fn start(&mut self) -> Result<(), OutputStartError<Self::Error>> {
        self.trace
            .starts
            .set(self.trace.starts.get() + 1);
        self.inner.start()
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        self.inner.stop()
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
        self.trace
            .writes
            .set(self.trace.writes.get() + 1);
        self.inner.set_targets(targets)
    }

    fn service(&mut self) {
        self.trace
            .services
            .set(self.trace.services.get() + 1);
        self.inner.service();
    }
}

type Processor = FrameProcessor<ObservedOutput>;

fn processor() -> (Processor, Rc<Trace>) {
    let trace = Rc::new(Trace::default());
    let output = ObservedOutput {
        inner: FakeOutput::inactive(),
        trace: Rc::clone(&trace),
    };
    let registers = RegisterService::new(0x1234, 18, output, true);

    (
        FrameProcessor::new(registers).expect("Valid register map"),
        trace,
    )
}

fn checksum(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

fn request(opcode: Opcode, index: u8, length: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = [0; MAX_FRAME_SIZE];
    let size = encode_frame(
        Header {
            opcode,
            index,
            length,
        },
        payload,
        FrameKind::Request,
        &mut bytes,
        &mut checksum,
    )
    .expect("Encode request");

    bytes[..size].to_vec()
}

fn poll(
    processor: &mut Processor,
    transport: &mut TestTransport,
    receive_budget: usize,
    transmit_budget: usize,
) -> FrameProgress {
    processor
        .poll(
            transport,
            receive_budget,
            transmit_budget,
            &mut checksum,
        )
        .expect("Successful processing")
}

fn assert_response(bytes: &[u8], index: u8, payload: &[u8]) {
    let frame = decode_frame(
        bytes,
        FrameKind::Response,
        &mut checksum,
    )
    .expect("Valid response framing and CRC");

    assert_eq!(
        frame.header,
        Header {
            opcode: Opcode::Ack,
            index,
            length: payload.len() as u16,
        },
    );
    assert_eq!(frame.payload, payload);
}

fn queue_read(
    processor: &mut Processor,
    transport: &mut TestTransport,
    index: u8,
    length: u16,
) {
    transport.feed(&request(
        Opcode::Read,
        index,
        length,
        &[],
    ));

    let progress = poll(processor, transport, MAX_FRAME_SIZE, 0);

    assert_eq!(progress.processed, 1);
    assert_eq!(progress.transmitted, 0);
    assert!(processor.response_pending());
    assert!(transport.outgoing.is_empty());
}

#[test]
fn idle_poll_services_backend_without_transmitting() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();

    assert_eq!(
        poll(&mut processor, &mut transport, 10, 10),
        FrameProgress {
            receive_empty: true,
            ..FrameProgress::default()
        },
    );

    assert_eq!(trace.services.get(), 1);
    assert_eq!(transport.read_calls, 1);
    assert_eq!(transport.write_calls, 0);
    assert!(!processor.response_pending());
}

#[test]
fn zero_budgets_still_service_backend_without_transport_access() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();
    transport.feed(&request(Opcode::Read, 1, 1, &[]));

    assert_eq!(
        poll(&mut processor, &mut transport, 0, 0),
        FrameProgress::default(),
    );

    assert_eq!(trace.services.get(), 1);
    assert_eq!(transport.incoming.len(), 8);
    assert_eq!(transport.read_calls, 0);
    assert_eq!(transport.write_calls, 0);
}

#[test]
fn fragmented_read_respects_receive_budget_and_queues_response() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();
    transport.feed(&request(Opcode::Read, 1, 1, &[]));

    let first = poll(&mut processor, &mut transport, 3, 20);
    assert_eq!(first.received, 3);
    assert_eq!(first.processed, 0);
    assert!(!first.receive_empty);
    assert!(!processor.response_pending());

    let second = poll(&mut processor, &mut transport, 20, 20);
    assert_eq!(second.received, 5);
    assert_eq!(second.processed, 1);
    assert_eq!(second.transmitted, 0);
    assert!(processor.response_pending());
    assert!(transport.outgoing.is_empty());

    let third = poll(&mut processor, &mut transport, 0, 20);
    assert_eq!(third.transmitted, 9);
    assert_eq!(third.received, 0);
    assert!(!processor.response_pending());

    assert_response(
        &transport.outgoing,
        1,
        &[STATUS_CALIBRATION_VALID],
    );
    assert_eq!(trace.services.get(), 3);
}

#[test]
fn write_then_read_preserves_request_and_response_order() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();

    let mut targets = [0; 8];
    targets[..4].copy_from_slice(&(-4_500_i32).to_le_bytes());
    targets[4..].copy_from_slice(&4_500_i32.to_le_bytes());

    transport.feed(&request(
        Opcode::Write,
        0x20,
        8,
        &targets,
    ));
    transport.feed(&request(Opcode::Read, 0x20, 8, &[]));

    // The WRITE queues its ACK before the following READ is consumed.
    let first = poll(&mut processor, &mut transport, 100, 100);
    assert_eq!(first.received, 16);
    assert_eq!(first.processed, 1);
    assert_eq!(first.transmitted, 0);
    assert_eq!(transport.incoming.len(), 8);
    assert_eq!(trace.writes.get(), 1);
    assert_eq!(trace.starts.get(), 0);
    assert!(processor.response_pending());
    assert!(transport.outgoing.is_empty());

    // Complete the write ACK, then process and queue the read response.
    let second = poll(&mut processor, &mut transport, 100, 100);
    assert_eq!(second.transmitted, 8);
    assert_eq!(second.received, 8);
    assert_eq!(second.processed, 1);
    assert!(processor.response_pending());
    assert_response(&transport.outgoing, 0x20, &[]);

    let third = poll(&mut processor, &mut transport, 0, 100);
    assert_eq!(third.transmitted, 16);
    assert_eq!(third.received, 0);
    assert_eq!(third.processed, 0);
    assert_eq!(transport.outgoing.len(), 24);
    assert_response(&transport.outgoing[..8], 0x20, &[]);
    assert_response(&transport.outgoing[8..], 0x20, &targets);
    assert!(!processor.response_pending());
    assert_eq!(trace.writes.get(), 1);
    assert_eq!(trace.services.get(), 3);
}

#[test]
fn pending_response_blocks_reception_without_losing_next_request() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();

    transport.feed(&request(Opcode::Read, 1, 1, &[]));
    transport.feed(&request(Opcode::Read, 4, 1, &[]));

    let first = poll(&mut processor, &mut transport, 100, 0);
    assert_eq!(first.received, 8);
    assert_eq!(first.processed, 1);
    assert_eq!(transport.incoming.len(), 8);
    let read_calls = transport.read_calls;

    assert_eq!(
        poll(&mut processor, &mut transport, 100, 0),
        FrameProgress::default(),
    );
    assert_eq!(transport.read_calls, read_calls);

    let partial = poll(&mut processor, &mut transport, 100, 3);
    assert_eq!(partial.transmitted, 3);
    assert_eq!(partial.received, 0);
    assert_eq!(transport.read_calls, read_calls);

    let next = poll(&mut processor, &mut transport, 100, 100);
    assert_eq!(next.transmitted, 6);
    assert_eq!(next.received, 8);
    assert_eq!(next.processed, 1);
    assert!(processor.response_pending());

    assert_response(
        &transport.outgoing,
        1,
        &[STATUS_CALIBRATION_VALID],
    );

    poll(&mut processor, &mut transport, 0, 100);

    assert_eq!(transport.outgoing.len(), 18);
    assert_response(&transport.outgoing[9..], 4, &[18]);
    assert!(!processor.response_pending());
    assert_eq!(trace.services.get(), 5);
}

#[test]
fn transmit_backpressure_preserves_response_and_services_backend() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();

    queue_read(&mut processor, &mut transport, 1, 1);
    transport
        .write_results
        .push_back(Ok(false));
    let read_calls = transport.read_calls;

    assert_eq!(
        poll(&mut processor, &mut transport, 100, 100),
        FrameProgress::default(),
    );

    assert!(processor.response_pending());
    assert!(transport.outgoing.is_empty());
    assert_eq!(transport.write_calls, 1);
    assert_eq!(transport.read_calls, read_calls);
    assert_eq!(trace.services.get(), 2);

    poll(&mut processor, &mut transport, 0, 100);

    assert_response(
        &transport.outgoing,
        1,
        &[STATUS_CALIBRATION_VALID],
    );
    assert!(!processor.response_pending());
}

#[test]
fn transmit_error_retains_suffix_without_replaying_accepted_byte() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();

    queue_read(&mut processor, &mut transport, 0, 2);
    transport
        .write_results
        .push_back(Ok(true));
    transport
        .write_results
        .push_back(Err(IoError::Write));
    let read_calls = transport.read_calls;

    assert_eq!(
        processor.poll(&mut transport, 100, 100, &mut checksum),
        Err(FrameProcessingError::Transmit(
            IoError::Write
        )),
    );

    assert_eq!(transport.outgoing.len(), 1);
    assert_eq!(transport.read_calls, read_calls);
    assert!(processor.response_pending());
    assert_eq!(trace.services.get(), 2);

    let progress = poll(&mut processor, &mut transport, 0, 100);

    assert_eq!(progress.transmitted, 9);
    assert_response(&transport.outgoing, 0, &[0x34, 0x12]);
    assert!(!processor.response_pending());
}

#[test]
fn receive_error_preserves_partial_request_until_caller_decides() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();
    let bytes = request(Opcode::Read, 1, 1, &[]);

    transport.feed(&bytes[..3]);
    transport
        .incoming
        .push_back(Err(IoError::Read));
    transport.feed(&bytes[3..]);

    assert_eq!(
        processor.poll(&mut transport, 100, 100, &mut checksum),
        Err(FrameProcessingError::Receive(
            IoError::Read
        )),
    );

    assert_eq!(trace.services.get(), 1);
    assert!(!processor.response_pending());
    assert!(transport.outgoing.is_empty());

    let progress = poll(&mut processor, &mut transport, 100, 100);
    assert_eq!(progress.received, 5);
    assert_eq!(progress.processed, 1);

    poll(&mut processor, &mut transport, 0, 100);

    assert_response(
        &transport.outgoing,
        1,
        &[STATUS_CALIBRATION_VALID],
    );
}

#[test]
fn corrupt_request_is_ignored_and_following_read_is_processed() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();

    let mut corrupt = request(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]);
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0x80;

    transport.feed(&corrupt);
    transport.feed(&request(Opcode::Read, 1, 1, &[]));

    let progress = poll(&mut processor, &mut transport, 100, 100);

    assert_eq!(progress.processed, 1);
    assert_eq!(trace.starts.get(), 0);
    assert_eq!(trace.writes.get(), 0);

    poll(&mut processor, &mut transport, 0, 100);

    assert_response(
        &transport.outgoing,
        1,
        &[STATUS_CALIBRATION_VALID],
    );
}

#[test]
fn timeout_discards_incomplete_request_before_new_input() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();
    let incomplete = request(Opcode::Set, 3, 1, &[OUTPUT_ENABLE]);

    transport.feed(&incomplete[..4]);

    let progress = poll(&mut processor, &mut transport, 100, 100);
    assert!(progress.receive_empty);
    assert_eq!(progress.processed, 0);

    processor.on_receive_timeout();
    transport.feed(&request(Opcode::Read, 1, 1, &[]));

    let progress = poll(&mut processor, &mut transport, 100, 100);
    assert_eq!(progress.processed, 1);
    assert_eq!(trace.starts.get(), 0);

    poll(&mut processor, &mut transport, 0, 100);

    assert_response(
        &transport.outgoing,
        1,
        &[STATUS_CALIBRATION_VALID],
    );
}

#[test]
fn timeout_preserves_complete_buffered_requests_and_pending_responses() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();

    // A plausible but corrupt outer frame makes the parser accumulate
    // both embedded READs before resynchronizing to the first one.
    let mut candidate = [0xFF; 80];
    candidate[..4].copy_from_slice(
        &Header {
            opcode: Opcode::Write,
            index: 0x20,
            length: 72,
        }
        .encode(),
    );

    candidate[4..12].copy_from_slice(&request(Opcode::Read, 1, 1, &[]));
    candidate[12..20].copy_from_slice(&request(Opcode::Read, 4, 1, &[]));

    // The last four bytes also form an incomplete SET header.
    candidate[76..80].copy_from_slice(&[3, 3, 1, 0]);

    assert_ne!(
        checksum(&candidate[..76]),
        u32::from_le_bytes(candidate[76..80].try_into().unwrap()),
    );

    transport.feed(&candidate);

    let first = poll(&mut processor, &mut transport, 100, 0);
    assert_eq!(first.received, 80);
    assert_eq!(first.processed, 1);
    assert!(processor.response_pending());
    assert!(transport.incoming.is_empty());

    processor.on_receive_timeout();

    // No transmit capacity: preserve the first response and timeout.
    let blocked = poll(&mut processor, &mut transport, 0, 0);
    assert_eq!(blocked.processed, 0);
    assert!(processor.response_pending());

    // Sending the first response permits the buffered second READ.
    let second = poll(&mut processor, &mut transport, 0, 100);
    assert_eq!(second.transmitted, 9);
    assert_eq!(second.processed, 1);
    assert!(processor.response_pending());

    assert_response(
        &transport.outgoing,
        1,
        &[STATUS_CALIBRATION_VALID],
    );

    // Only after complete requests are drained may the incomplete SET
    // remainder be discarded.
    let third = poll(&mut processor, &mut transport, 0, 100);
    assert_eq!(third.transmitted, 9);
    assert_eq!(third.processed, 0);
    assert!(!processor.response_pending());
    assert_response(&transport.outgoing[9..], 4, &[18]);

    transport.feed(&request(Opcode::Read, 3, 1, &[]));
    poll(&mut processor, &mut transport, 100, 0);
    poll(&mut processor, &mut transport, 0, 100);

    assert_response(&transport.outgoing[18..], 3, &[0]);
    assert_eq!(trace.starts.get(), 0);
    assert_eq!(trace.writes.get(), 0);
}

#[test]
fn full_target_response_survives_small_receive_and_transmit_budgets() {
    let (mut processor, trace) = processor();
    let mut transport = TestTransport::default();
    let mut payload = [0; 72];

    for (index, bytes) in payload.chunks_exact_mut(4).enumerate() {
        let value = -8_500 + index as i32 * 1_000;
        bytes.copy_from_slice(&value.to_le_bytes());
    }

    transport.feed(&request(
        Opcode::Write,
        0x20,
        72,
        &payload,
    ));
    transport.feed(&request(Opcode::Read, 0x20, 72, &[]));

    let mut received = 0;
    let mut processed = 0;

    // Receive the complete WRITE in small chunks. Its ACK blocks the READ.
    for _ in 0..32 {
        let progress = poll(&mut processor, &mut transport, 7, 0);
        assert!(progress.received <= 7);
        assert_eq!(progress.transmitted, 0);
        received += progress.received;
        processed += progress.processed;

        if processor.response_pending() {
            break;
        }
    }

    assert!(processor.response_pending());
    assert_eq!(received, 80);
    assert_eq!(processed, 1);
    assert_eq!(transport.incoming.len(), 8);
    assert_eq!(trace.writes.get(), 1);
    assert_eq!(trace.starts.get(), 0);
    assert!(transport.outgoing.is_empty());

    // Drain the eight-byte ACK without receiving the next request.
    let mut ack_transmitted = 0;
    for _ in 0..2 {
        let progress = poll(&mut processor, &mut transport, 0, 7);
        assert_eq!(progress.received, 0);
        assert_eq!(progress.processed, 0);
        assert!(progress.transmitted <= 7);
        ack_transmitted += progress.transmitted;
        if !processor.response_pending() {
            break;
        }
    }
    assert_eq!(ack_transmitted, 8);
    assert!(!processor.response_pending());
    assert_response(&transport.outgoing, 0x20, &[]);
    assert_eq!(transport.incoming.len(), 8);

    // Receive the READ using the same small receive budget.
    for _ in 0..2 {
        let progress = poll(&mut processor, &mut transport, 7, 0);
        assert!(progress.received <= 7);
        assert_eq!(progress.transmitted, 0);
        received += progress.received;
        processed += progress.processed;
        if processor.response_pending() {
            break;
        }
    }
    assert_eq!(received, 88);
    assert_eq!(processed, 2);
    assert!(processor.response_pending());
    assert!(transport.incoming.is_empty());
    assert_eq!(transport.outgoing.len(), 8);

    let mut transmitted = 0;
    for _ in 0..32 {
        let progress = poll(&mut processor, &mut transport, 0, 7);
        assert!(progress.transmitted <= 7);
        assert_eq!(progress.received, 0);
        assert_eq!(progress.processed, 0);
        transmitted += progress.transmitted;
        if !processor.response_pending() {
            break;
        }
    }

    assert!(!processor.response_pending());
    assert_eq!(transmitted, 80);
    assert_eq!(transport.outgoing.len(), 88);
    assert_response(&transport.outgoing[..8], 0x20, &[]);
    assert_response(&transport.outgoing[8..], 0x20, &payload);
    assert_eq!(trace.writes.get(), 1);
    assert_eq!(trace.starts.get(), 0);
}
