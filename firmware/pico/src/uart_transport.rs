//! Nonblocking UART implementation of the protocol byte transport.

use hexaroam_servo_protocol::ByteTransport;
use rp_pico::hal::uart::{
    Enabled, ReadErrorType, UartDevice, UartPeripheral, ValidUartPinout,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UartTransportError {
    Overrun,
    Break,
    Parity,
    Framing,
}

pub struct UartTransport<U: UartDevice, P: ValidUartPinout<U>> {
    uart: UartPeripheral<Enabled, U, P>,
}

impl<U: UartDevice, P: ValidUartPinout<U>> UartTransport<U, P> {
    /// Takes exclusive ownership of an already configured UART.
    pub fn new(uart: UartPeripheral<Enabled, U, P>) -> Self {
        Self { uart }
    }
}

impl<U: UartDevice, P: ValidUartPinout<U>> ByteTransport
    for UartTransport<U, P>
{
    type Error = UartTransportError;

    fn try_read(&mut self) -> Result<Option<u8>, Self::Error> {
        let mut byte = [0];

        match self.uart.read_raw(&mut byte) {
            Ok(count) => Ok(if count == 1 { Some(byte[0]) } else { None }),
            Err(nb::Error::WouldBlock) => Ok(None),
            Err(nb::Error::Other(error)) => {
                // The HAL consumed the erroneous receive entry.
                // Do not pass it to the frame parser as a valid byte.
                Err(match error.err_type {
                    ReadErrorType::Overrun => UartTransportError::Overrun,
                    ReadErrorType::Break => UartTransportError::Break,
                    ReadErrorType::Parity => UartTransportError::Parity,
                    ReadErrorType::Framing => UartTransportError::Framing,
                })
            }
        }
    }

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        match self.uart.write_raw(&[byte]) {
            Ok(remaining) => Ok(remaining.is_empty()),
            Err(nb::Error::WouldBlock) => Ok(false),

            // The HAL's ordinary write error type is Infallible.
            Err(nb::Error::Other(error)) => match error {},
        }
    }
}
