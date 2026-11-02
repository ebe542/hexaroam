extern crate std;

use self::std::{collections::VecDeque, vec::Vec};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Disconnected,
    UnexpectedRead,
}

#[derive(Default)]
struct TestTransport {
    written: Vec<u8>,
    outcomes: VecDeque<Result<bool, TestError>>,
    write_calls: usize,
    read_calls: usize,
}

impl ByteTransport for TestTransport {
    type Error = TestError;

    fn try_read(&mut self) -> Result<Option<u8>, Self::Error> {
        self.read_calls += 1;
        Err(TestError::UnexpectedRead)
    }

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        self.write_calls += 1;

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
fn empty_buffer_does_not_access_transport() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    assert!(!buffer.is_pending());
    assert_eq!(buffer.remaining(), 0);
    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(0)
    );
    assert_eq!(transport.write_calls, 0);
    assert_eq!(transport.read_calls, 0);
}

#[test]
fn busy_queue_preserves_response_and_progress() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    buffer.queue(&[1, 2, 3]).unwrap();
    assert_eq!(buffer.service(&mut transport, 1), Ok(1));

    assert_eq!(
        buffer.queue(&[9]),
        Err(ResponseQueueError::Busy)
    );
    assert_eq!(
        buffer.queue(&[]),
        Err(ResponseQueueError::Busy)
    );
    assert_eq!(
        buffer.queue(&[9; 5]),
        Err(ResponseQueueError::Busy)
    );

    assert_eq!(buffer.remaining(), 2);
    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(2)
    );
    assert_eq!(transport.written.as_slice(), &[1, 2, 3]);
    assert!(!buffer.is_pending());
}

#[test]
fn empty_response_is_rejected_without_occupying_buffer() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    assert_eq!(
        buffer.queue(&[]),
        Err(ResponseQueueError::Empty)
    );
    assert!(!buffer.is_pending());
    assert_eq!(buffer.remaining(), 0);

    assert_eq!(buffer.queue(&[7]), Ok(()));
    assert_eq!(buffer.service(&mut transport, 1), Ok(1));
    assert_eq!(transport.written.as_slice(), &[7]);
}

#[test]
fn oversized_response_is_rejected_without_occupying_buffer() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    assert_eq!(
        buffer.queue(&[1, 2, 3, 4, 5]),
        Err(ResponseQueueError::TooLarge),
    );
    assert!(!buffer.is_pending());
    assert_eq!(buffer.remaining(), 0);

    assert_eq!(buffer.queue(&[7, 8]), Ok(()));
    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(2)
    );
    assert_eq!(transport.written.as_slice(), &[7, 8]);
}

#[test]
fn service_respects_budget_and_retains_byte_order() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    buffer.queue(&[1, 2, 3, 4]).unwrap();

    assert_eq!(buffer.service(&mut transport, 2), Ok(2));
    assert_eq!(transport.write_calls, 2);
    assert_eq!(transport.written.as_slice(), &[1, 2]);
    assert_eq!(buffer.remaining(), 2);
    assert!(buffer.is_pending());

    assert_eq!(buffer.service(&mut transport, 1), Ok(1));
    assert_eq!(transport.written.as_slice(), &[1, 2, 3]);
    assert_eq!(buffer.remaining(), 1);

    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(1)
    );
    assert_eq!(
        transport.written.as_slice(),
        &[1, 2, 3, 4]
    );
    assert_eq!(transport.write_calls, 4);
    assert_eq!(transport.read_calls, 0);
    assert!(!buffer.is_pending());
}

#[test]
fn unavailable_transport_preserves_initial_and_partial_progress() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    transport.outcomes.push_back(Ok(false));
    transport.outcomes.push_back(Ok(true));
    transport.outcomes.push_back(Ok(false));

    buffer.queue(&[1, 2, 3]).unwrap();

    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(0)
    );
    assert_eq!(transport.write_calls, 1);
    assert_eq!(buffer.remaining(), 3);
    assert!(transport.written.is_empty());

    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(1)
    );
    assert_eq!(transport.write_calls, 3);
    assert_eq!(buffer.remaining(), 2);
    assert_eq!(transport.written.as_slice(), &[1]);

    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(2)
    );
    assert_eq!(transport.write_calls, 5);
    assert_eq!(transport.written.as_slice(), &[1, 2, 3]);
    assert!(!buffer.is_pending());
}

#[test]
fn error_before_first_byte_preserves_complete_response() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    transport
        .outcomes
        .push_back(Err(TestError::Disconnected));

    buffer.queue(&[1, 2, 3]).unwrap();

    assert_eq!(
        buffer.service(&mut transport, 10),
        Err(TestError::Disconnected),
    );
    assert_eq!(buffer.remaining(), 3);
    assert!(buffer.is_pending());
    assert!(transport.written.is_empty());
    assert_eq!(transport.write_calls, 1);

    assert_eq!(
        buffer.queue(&[9]),
        Err(ResponseQueueError::Busy)
    );

    // Retry is an explicit caller decision.
    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(3)
    );
    assert_eq!(transport.written.as_slice(), &[1, 2, 3]);
    assert!(!buffer.is_pending());
}

#[test]
fn error_after_prefix_retains_only_unsent_suffix() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    transport.outcomes.push_back(Ok(true));
    transport
        .outcomes
        .push_back(Err(TestError::Disconnected));

    buffer.queue(&[1, 2, 3]).unwrap();

    assert_eq!(
        buffer.service(&mut transport, 10),
        Err(TestError::Disconnected),
    );
    assert_eq!(transport.written.as_slice(), &[1]);
    assert_eq!(buffer.remaining(), 2);
    assert_eq!(transport.write_calls, 2);

    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(2)
    );
    assert_eq!(transport.written.as_slice(), &[1, 2, 3]);
    assert_eq!(transport.write_calls, 4);
    assert!(!buffer.is_pending());
}

#[test]
fn zero_budget_does_not_access_transport_or_advance_response() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    buffer.queue(&[1, 2, 3]).unwrap();

    assert_eq!(buffer.service(&mut transport, 0), Ok(0));
    assert_eq!(buffer.remaining(), 3);
    assert!(buffer.is_pending());
    assert_eq!(transport.write_calls, 0);
    assert_eq!(transport.read_calls, 0);
}

#[test]
fn completed_buffer_accepts_another_shorter_response() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();

    buffer.queue(&[1, 2, 3, 4]).unwrap();
    assert_eq!(buffer.service(&mut transport, 4), Ok(4));

    assert!(!buffer.is_pending());
    assert_eq!(buffer.remaining(), 0);

    buffer.queue(&[8, 9]).unwrap();
    assert_eq!(buffer.remaining(), 2);
    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(2)
    );

    assert_eq!(
        transport.written.as_slice(),
        &[1, 2, 3, 4, 8, 9]
    );
    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(0)
    );
    assert_eq!(transport.write_calls, 6);
}

#[test]
fn queue_copies_bytes_before_caller_reuses_input() {
    let mut buffer = ResponseBuffer::<4>::new();
    let mut transport = TestTransport::default();
    let mut input = [1, 2, 3];

    buffer.queue(&input).unwrap();
    input.fill(9);

    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(3)
    );
    assert_eq!(transport.written.as_slice(), &[1, 2, 3]);
}

#[test]
fn zero_capacity_rejects_data_and_services_without_io() {
    let mut buffer = ResponseBuffer::<0>::new();
    let mut transport = TestTransport::default();

    assert_eq!(
        buffer.queue(&[]),
        Err(ResponseQueueError::Empty)
    );
    assert_eq!(
        buffer.queue(&[1]),
        Err(ResponseQueueError::TooLarge)
    );
    assert_eq!(
        buffer.service(&mut transport, 10),
        Ok(0)
    );

    assert!(!buffer.is_pending());
    assert_eq!(buffer.remaining(), 0);
    assert_eq!(transport.write_calls, 0);
    assert_eq!(transport.read_calls, 0);
}
