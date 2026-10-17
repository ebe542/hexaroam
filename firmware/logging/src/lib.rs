#![no_std]

//! Buffered logging with a nonblocking byte output.

/// The backend must return immediately.
///
/// Ok(true): byte accepted.
/// Ok(false): output currently full; byte not accepted.
/// Err: byte not accepted.
pub trait ByteOutput {
    type Error;

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Debug,
    Error,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LogError {
    BufferFull,
}

pub struct Logger<O: ByteOutput, const CAPACITY: usize> {
    output: O,
    buffer: [u8; CAPACITY],
    head: usize,
    len: usize,
    dropped_messages: u32,
}

impl<O: ByteOutput, const CAPACITY: usize> Logger<O, CAPACITY> {
    pub fn new(output: O) -> Self {
        Self {
            output,
            buffer: [0; CAPACITY],
            head: 0,
            len: 0,
            dropped_messages: 0,
        }
    }

    /// Queues a complete "LEVEL message\n" line.
    ///
    /// On insufficient space, queues nothing and increments
    /// the saturating dropped-message counter.
    pub fn log(
        &mut self,
        level: LogLevel,
        message: &str,
    ) -> Result<(), LogError> {
        let prefix: &[u8] = match level {
            LogLevel::Info => b"INFO",
            LogLevel::Debug => b"DEBUG",
            LogLevel::Error => b"ERROR",
        };

        // One space after the level and LF after the message.
        let overhead = prefix.len() + 2;
        let available = CAPACITY - self.len;

        // Check the complete message before modifying the buffer.
        // Subtraction avoids overflowing a combined message length.
        if available < overhead || message.len() > available - overhead {
            self.dropped_messages = self.dropped_messages.saturating_add(1);

            return Err(LogError::BufferFull);
        }

        for &byte in prefix
            .iter()
            .chain(b" ".iter())
            .chain(message.as_bytes().iter())
            .chain(b"\n".iter())
        {
            let tail = (self.head + self.len) % CAPACITY;
            self.buffer[tail] = byte;
            self.len += 1;
        }

        Ok(())
    }

    /// Attempts to transfer at most byte_budget bytes.
    ///
    /// Stops immediately when the output is full or reports an error.
    /// Only accepted bytes are removed from the queue.
    pub fn service(&mut self, byte_budget: usize) -> Result<usize, O::Error> {
        let mut transferred = 0;

        while transferred < byte_budget && self.len > 0 {
            let byte = self.buffer[self.head];

            if !self.output.try_write(byte)? {
                break;
            }

            // Remove a byte only after the backend accepted it.
            self.head = (self.head + 1) % CAPACITY;
            self.len -= 1;
            transferred += 1;
        }

        Ok(transferred)
    }

    pub fn queued_bytes(&self) -> usize {
        self.len
    }

    pub fn dropped_messages(&self) -> u32 {
        self.dropped_messages
    }
}

mod bounded_writer;

pub use bounded_writer::BoundedWriter;

#[cfg(test)]
extern crate std;

#[cfg(test)]
#[path = "test_logging.rs"]
mod tests;
