//! Direct output with a shared, finite write-attempt budget.

use core::fmt;

use crate::ByteOutput;

pub struct BoundedWriter<'a, O: ByteOutput> {
    output: &'a mut O,
    remaining_attempts: usize,
}

impl<'a, O: ByteOutput> BoundedWriter<'a, O> {
    pub fn new(output: &'a mut O, max_attempts: usize) -> Self {
        Self {
            output,
            remaining_attempts: max_attempts,
        }
    }
}

impl<O: ByteOutput> fmt::Write for BoundedWriter<'_, O> {
    /// Retries busy output within the shared attempt budget.
    ///
    /// Backend errors terminate output and exhaust the budget.
    /// Bytes already accepted are not rolled back.
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for &byte in text.as_bytes() {
            loop {
                if self.remaining_attempts == 0 {
                    return Err(fmt::Error);
                }

                self.remaining_attempts -= 1;

                match self.output.try_write(byte) {
                    Ok(true) => break,
                    Ok(false) => continue,
                    Err(_) => {
                        self.remaining_attempts = 0;
                        return Err(fmt::Error);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "test_bounded_writer.rs"]
mod tests;
