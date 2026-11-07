#![no_std]

//! Driver for 32 KiB EEPROMs of the 24xx256 family.

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::{Error, ErrorKind, I2c, NoAcknowledgeSource};

pub const CAPACITY: usize = 32 * 1024;
pub const PAGE_SIZE: usize = 64;
// The datasheet specifies a maximum write cycle of 5 ms.
// Allow ten 1 ms retry intervals before reporting a timeout.
const WRITE_POLL_INTERVAL_US: u32 = 1_000;
const WRITE_POLL_RETRIES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EepromError<E> {
    InvalidDeviceAddress,
    OutOfBounds,
    WriteTimeout,
    Bus(E),
}

pub struct Eeprom<I> {
    i2c: I,
    address: u8,
}

impl<I: I2c> Eeprom<I> {
    /// Creates a driver without accessing the bus.
    ///
    /// The seven-bit address must be between 0x50 and 0x57.
    pub fn new(i2c: I, address: u8) -> Result<Self, EepromError<I::Error>> {
        if !(0x50..=0x57).contains(&address) {
            return Err(EepromError::InvalidDeviceAddress);
        }

        Ok(Self { i2c, address })
    }

    /// Reads bytes using a combined address-write/read transaction.
    ///
    /// Reads must not wrap around the end of the device.
    /// An empty read is allowed at offsets 0..=CAPACITY and
    /// performs no bus access.
    ///
    /// Range errors leave output unchanged.
    /// After a bus error, output must not be interpreted.
    pub fn read(
        &mut self,
        offset: u16,
        output: &mut [u8],
    ) -> Result<(), EepromError<I::Error>> {
        let start = usize::from(offset);

        if start > CAPACITY || output.len() > CAPACITY - start {
            return Err(EepromError::OutOfBounds);
        }

        if output.is_empty() {
            return Ok(());
        }

        self.i2c
            .write_read(
                self.address,
                &offset.to_be_bytes(),
                output,
            )
            .map_err(EepromError::Bus)
    }

    /// Writes bytes without crossing physical page boundaries.
    ///
    /// The complete range is checked before accessing the bus.
    /// Empty writes within bounds perform no bus access.
    ///
    /// After each page write, readiness is checked by sending only
    /// its two address bytes, without payload. This does not write
    /// EEPROM contents.
    ///
    /// Address NACKs, including an unknown NACK source, are retried
    /// after 1 ms, up to ten delays. Other errors abort immediately.
    /// A data NACK is not treated as a busy indication.
    ///
    /// Success means all transfers completed and the device is ready.
    /// It does not verify stored contents or detect write protection.
    /// On error, some data may already have been written.
    /// Retry limits do not bound a blocking I2C operation itself.
    pub fn write(
        &mut self,
        offset: u16,
        data: &[u8],
        delay: &mut impl DelayNs,
    ) -> Result<(), EepromError<I::Error>> {
        let start = usize::from(offset);

        if start > CAPACITY || data.len() > CAPACITY - start {
            return Err(EepromError::OutOfBounds);
        }

        let mut written = 0;
        let mut buffer = [0_u8; 2 + PAGE_SIZE];

        while written < data.len() {
            let position = start + written;
            let address_bytes = (position as u16).to_be_bytes();

            let remaining_in_page = PAGE_SIZE - position % PAGE_SIZE;
            let chunk_length = remaining_in_page.min(data.len() - written);

            buffer[..2].copy_from_slice(&address_bytes);
            buffer[2..2 + chunk_length]
                .copy_from_slice(&data[written..written + chunk_length]);

            self.i2c
                .write(
                    self.address,
                    &buffer[..2 + chunk_length],
                )
                .map_err(EepromError::Bus)?;

            let mut retries = 0;

            loop {
                // Address bytes alone set the internal pointer.
                // Without payload, no EEPROM write cycle is started.
                match self
                    .i2c
                    .write(self.address, &address_bytes)
                {
                    Ok(()) => break,
                    Err(error) => match error.kind() {
                        ErrorKind::NoAcknowledge(
                            NoAcknowledgeSource::Address
                            | NoAcknowledgeSource::Unknown,
                        ) => {
                            if retries == WRITE_POLL_RETRIES {
                                return Err(EepromError::WriteTimeout);
                            }

                            delay.delay_us(WRITE_POLL_INTERVAL_US);
                            retries += 1;
                        }
                        _ => return Err(EepromError::Bus(error)),
                    },
                }
            }

            written += chunk_length;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "test_eeprom.rs"]
mod tests;

#[cfg(test)]
#[path = "test_write.rs"]
mod write_tests;
