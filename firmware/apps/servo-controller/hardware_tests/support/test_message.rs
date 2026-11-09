//! Bounded formatting for manual hardware-test log messages.

use core::fmt::{self, Write};
use hexaroam_logging::{ByteOutput, LogLevel, Logger};

struct TestMessage {
    bytes: [u8; 128],
    len: usize,
}

impl Write for TestMessage {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self
            .len
            .checked_add(text.len())
            .ok_or(fmt::Error)?;
        let destination = self
            .bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?;

        destination.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

pub fn log_message<O: ByteOutput, const CAPACITY: usize>(
    logger: &mut Logger<O, CAPACITY>,
    arguments: fmt::Arguments<'_>,
) -> Result<(), &'static str> {
    let mut message = TestMessage {
        bytes: [0; 128],
        len: 0,
    };

    message
        .write_fmt(arguments)
        .map_err(|_| "Test message formatting failed")?;

    let text = core::str::from_utf8(&message.bytes[..message.len])
        .map_err(|_| "Invalid test message encoding")?;

    logger
        .log(LogLevel::Info, text)
        .map_err(|_| "Test log buffer full")
}
