//! Nonblocking UART backend for buffered logging.

use core::convert::Infallible;

use hexaroam_logging::ByteOutput;
use rp_pico::hal::uart::{
    Enabled, UartDevice, UartPeripheral, ValidUartPinout,
};

pub struct UartLogOutput<U: UartDevice, P: ValidUartPinout<U>> {
    uart: UartPeripheral<Enabled, U, P>,
}

impl<U: UartDevice, P: ValidUartPinout<U>> UartLogOutput<U, P> {
    pub fn new(uart: UartPeripheral<Enabled, U, P>) -> Self {
        Self { uart }
    }
}

impl<U: UartDevice, P: ValidUartPinout<U>> ByteOutput for UartLogOutput<U, P> {
    type Error = Infallible;

    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
        match self.uart.write_raw(&[byte]) {
            Ok(remaining) => Ok(remaining.is_empty()),

            // The HAL's write error is Infallible.
            // Therefore, an error here can only mean WouldBlock.
            Err(_) => Ok(false),
        }
    }
}
