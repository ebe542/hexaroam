use super::*;
use std::vec::Vec;

#[derive(Debug, PartialEq, Eq)]
enum TestError {
    Failed,
}

#[derive(Default)]
struct TestOutput {
    bytes: Vec<u8>,
    blocked: bool,
    fail_after: Option<usize>,
}

impl ByteOutput for TestOutput {
    type Error = TestError;

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        if self.fail_after == Some(self.bytes.len()) {
            return Err(TestError::Failed);
        }

        if self.blocked {
            return Ok(false);
        }

        self.bytes.push(byte);
        Ok(true)
    }
}

#[test]
fn construction_starts_empty() {
    let logger = Logger::<_, 16>::new(TestOutput::default());

    assert_eq!(logger.queued_bytes(), 0);
    assert_eq!(logger.dropped_messages(), 0);
    assert!(logger.output.bytes.is_empty());
}

#[test]
fn logging_only_queues_until_serviced() {
    let mut logger = Logger::<_, 16>::new(TestOutput::default());

    assert_eq!(logger.log(LogLevel::Info, "a"), Ok(()));
    assert_eq!(logger.queued_bytes(), 7);
    assert!(logger.output.bytes.is_empty());

    assert_eq!(logger.service(16), Ok(7));
    assert_eq!(logger.output.bytes, b"INFO a\n");
    assert_eq!(logger.queued_bytes(), 0);
}

#[test]
fn levels_are_formatted_and_sent_in_order() {
    let mut logger = Logger::<_, 64>::new(TestOutput::default());

    logger
        .log(LogLevel::Info, "one")
        .unwrap();
    logger
        .log(LogLevel::Debug, "two")
        .unwrap();
    logger
        .log(LogLevel::Error, "three")
        .unwrap();

    logger.service(64).unwrap();

    assert_eq!(
        logger.output.bytes,
        b"INFO one\nDEBUG two\nERROR three\n",
    );
}

#[test]
fn full_buffer_drops_the_whole_new_message() {
    let mut logger = Logger::<_, 7>::new(TestOutput::default());

    assert_eq!(logger.log(LogLevel::Info, "a"), Ok(()));

    assert_eq!(
        logger.log(LogLevel::Error, "lost"),
        Err(LogError::BufferFull),
    );

    assert_eq!(logger.queued_bytes(), 7);
    assert_eq!(logger.dropped_messages(), 1);

    logger.service(7).unwrap();

    assert_eq!(logger.output.bytes, b"INFO a\n");
}

#[test]
fn utf8_capacity_is_counted_in_bytes() {
    let mut logger = Logger::<_, 8>::new(TestOutput::default());

    assert_eq!(logger.log(LogLevel::Info, "ä"), Ok(()));
    assert_eq!(logger.queued_bytes(), 8);

    logger.service(8).unwrap();

    assert_eq!(
        logger.output.bytes,
        "INFO ä\n".as_bytes(),
    );
}

#[test]
fn service_respects_zero_and_partial_budgets() {
    let mut logger = Logger::<_, 16>::new(TestOutput::default());

    logger.log(LogLevel::Info, "a").unwrap();

    assert_eq!(logger.service(0), Ok(0));
    assert!(logger.output.bytes.is_empty());

    assert_eq!(logger.service(3), Ok(3));
    assert_eq!(logger.output.bytes, b"INF");
    assert_eq!(logger.queued_bytes(), 4);

    assert_eq!(logger.service(16), Ok(4));
    assert_eq!(logger.output.bytes, b"INFO a\n");
}

#[test]
fn blocked_output_preserves_queued_data() {
    let mut logger = Logger::<_, 16>::new(TestOutput {
        blocked: true,
        ..TestOutput::default()
    });

    logger.log(LogLevel::Info, "a").unwrap();

    assert_eq!(logger.service(16), Ok(0));
    assert_eq!(logger.queued_bytes(), 7);
    assert_eq!(logger.dropped_messages(), 0);

    logger.output.blocked = false;

    assert_eq!(logger.service(16), Ok(7));
    assert_eq!(logger.output.bytes, b"INFO a\n");
}

#[test]
fn ring_wrap_preserves_message_order() {
    let mut logger = Logger::<_, 12>::new(TestOutput::default());

    logger.log(LogLevel::Info, "a").unwrap();
    assert_eq!(logger.service(5), Ok(5));

    logger.log(LogLevel::Error, "").unwrap();
    assert_eq!(logger.service(12), Ok(9));

    assert_eq!(logger.output.bytes, b"INFO a\nERROR \n",);
    assert_eq!(logger.queued_bytes(), 0);
}

#[test]
fn output_error_preserves_the_unaccepted_byte_and_remainder() {
    let mut logger = Logger::<_, 16>::new(TestOutput {
        fail_after: Some(3),
        ..TestOutput::default()
    });

    logger.log(LogLevel::Info, "a").unwrap();

    assert_eq!(
        logger.service(16),
        Err(TestError::Failed)
    );
    assert_eq!(logger.output.bytes, b"INF");
    assert_eq!(logger.queued_bytes(), 4);

    logger.output.fail_after = None;

    assert_eq!(logger.service(16), Ok(4));
    assert_eq!(logger.output.bytes, b"INFO a\n");
}

#[test]
fn zero_capacity_rejects_messages_without_panicking() {
    let mut logger = Logger::<_, 0>::new(TestOutput::default());

    assert_eq!(
        logger.log(LogLevel::Info, "a"),
        Err(LogError::BufferFull),
    );
    assert_eq!(logger.dropped_messages(), 1);
    assert_eq!(logger.queued_bytes(), 0);
    assert_eq!(logger.service(16), Ok(0));
}
