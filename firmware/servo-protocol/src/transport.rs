//! Nonblocking byte transport, independent of UART hardware.

pub trait ByteTransport {
    type Error;

    /// Returns one received byte, or None when no byte is available.
    ///
    /// Must return without waiting for additional input.
    fn try_read(&mut self) -> Result<Option<u8>, Self::Error>;

    /// Attempts to transmit one byte without waiting.
    ///
    /// Ok(true): byte accepted by the transport.
    /// Ok(false): temporarily unavailable; byte not accepted.
    /// Err: byte not accepted.
    ///
    /// Acceptance does not confirm transmission on the physical wire.
    fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error>;
}
