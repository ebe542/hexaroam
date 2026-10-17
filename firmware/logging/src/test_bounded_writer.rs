use super::*;
use core::fmt::Write;
use std::vec::Vec;

#[derive(Default)]
struct TestOutput {
    bytes: Vec<u8>,
    attempts: usize,
    busy_attempts: usize,
    fail: bool,
}

impl ByteOutput for TestOutput {
    type Error = ();

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        self.attempts += 1;

        if self.fail {
            return Err(());
        }

        if self.busy_attempts > 0 {
            self.busy_attempts -= 1;
            return Ok(false);
        }

        self.bytes.push(byte);
        Ok(true)
    }
}

#[test]
fn writes_utf8_bytes_with_an_exact_budget() {
    let mut output = TestOutput::default();
    let text = "ä\n";

    let mut writer = BoundedWriter::new(&mut output, text.len());

    assert_eq!(writer.write_str(text), Ok(()));
    assert_eq!(output.bytes, text.as_bytes());
    assert_eq!(output.attempts, text.len());
}

#[test]
fn empty_text_needs_no_budget() {
    let mut output = TestOutput::default();
    let mut writer = BoundedWriter::new(&mut output, 0);

    assert_eq!(writer.write_str(""), Ok(()));
    assert_eq!(output.attempts, 0);
}

#[test]
fn zero_budget_rejects_nonempty_text_without_accessing_output() {
    let mut output = TestOutput::default();
    let mut writer = BoundedWriter::new(&mut output, 0);

    assert_eq!(writer.write_str("a"), Err(fmt::Error));
    assert_eq!(output.attempts, 0);
    assert!(output.bytes.is_empty());
}

#[test]
fn permanently_busy_output_stops_at_the_budget() {
    let mut output = TestOutput {
        busy_attempts: usize::MAX,
        ..TestOutput::default()
    };
    let mut writer = BoundedWriter::new(&mut output, 3);

    assert_eq!(writer.write_str("a"), Err(fmt::Error));
    assert_eq!(output.attempts, 3);
    assert!(output.bytes.is_empty());
}

#[test]
fn retries_a_byte_without_duplicating_it() {
    let mut output = TestOutput {
        busy_attempts: 2,
        ..TestOutput::default()
    };
    let mut writer = BoundedWriter::new(&mut output, 4);

    assert_eq!(writer.write_str("ab"), Ok(()));
    assert_eq!(output.attempts, 4);
    assert_eq!(output.bytes, b"ab");
}

#[test]
fn budget_is_shared_across_calls_and_preserves_partial_output() {
    let mut output = TestOutput::default();
    let mut writer = BoundedWriter::new(&mut output, 3);

    assert_eq!(writer.write_str("ab"), Ok(()));
    assert_eq!(writer.write_str("cd"), Err(fmt::Error));
    assert_eq!(writer.write_str("e"), Err(fmt::Error));

    assert_eq!(output.attempts, 3);
    assert_eq!(output.bytes, b"abc");
}

#[test]
fn backend_error_prevents_further_attempts() {
    let mut output = TestOutput {
        fail: true,
        ..TestOutput::default()
    };
    let mut writer = BoundedWriter::new(&mut output, 10);

    assert_eq!(writer.write_str("a"), Err(fmt::Error));
    assert_eq!(writer.write_str("b"), Err(fmt::Error));

    assert_eq!(output.attempts, 1);
    assert!(output.bytes.is_empty());
}
