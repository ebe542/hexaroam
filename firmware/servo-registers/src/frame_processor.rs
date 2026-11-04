//! Bounded request processing over a nonblocking byte transport.

use hexaroam_servo_protocol::{
    ByteTransport, FrameError, FrameKind, MAX_FRAME_SIZE, MAX_PAYLOAD_SIZE,
    ParserError, ResponseBuffer, ResponseQueueError, StreamParser,
    encode_frame,
};

use crate::{
    CalibrationCompletionError, CalibrationSaveResult, OutputBackend,
    PendingCalibration, REGISTER_REGIONS, RegisterService, ServiceError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameProgress {
    /// Bytes consumed from the receive transport during this call.
    pub received: usize,

    /// Response bytes accepted by the transport during this call.
    pub transmitted: usize,

    /// Complete requests passed to the register service.
    pub processed: usize,

    /// A receive attempt reported that no byte was available.
    ///
    /// False also covers calls that did not attempt reception.
    pub receive_empty: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FrameProcessingError<E> {
    Receive(E),
    Transmit(E),
    Parser(ParserError),
    Service(ServiceError),
    Encoding(FrameError),
    Queue(ResponseQueueError),
}

pub struct FrameProcessor<O: OutputBackend> {
    registers: RegisterService<O>,
    parser: StreamParser<'static>,
    response: ResponseBuffer<MAX_FRAME_SIZE>,
    receive_timeout_pending: bool,
}

impl<O: OutputBackend> FrameProcessor<O> {
    pub fn new(registers: RegisterService<O>) -> Result<Self, ParserError> {
        Ok(Self {
            registers,
            parser: StreamParser::new(FrameKind::Request, &REGISTER_REGIONS)?,
            response: ResponseBuffer::new(),
            receive_timeout_pending: false,
        })
    }

    /// Reports response bytes not yet accepted by the transport.
    ///
    /// False does not guarantee physical wire completion.
    pub fn response_pending(&self) -> bool {
        self.response.is_pending()
    }

    /// Records an externally detected receive timeout.
    ///
    /// Complete buffered requests must be processed before clearing
    /// the incomplete remainder. Pending responses are not discarded.
    ///
    /// The caller determines timing and must account for unread
    /// transport bytes; this method does not inspect UART hardware.
    pub fn on_receive_timeout(&mut self) {
        self.receive_timeout_pending = true;
    }

    /// Services output, pending transmission and bounded reception.
    ///
    /// Order:
    /// 1. Service the register backend once, including when budgets are zero.
    /// 2. Transmit an existing response up to transmit_budget bytes.
    /// 3. If a response remains pending, return without receiving.
    /// 4. Drain complete buffered requests before reading more bytes.
    /// 5. Apply a pending timeout only to the incomplete parser remainder.
    /// 6. Consume at most receive_budget new bytes, processing full requests.
    ///
    /// ACK and NACK responses are encoded and queued before processing more input.
    /// A newly queued response is transmitted on a later poll call.
    ///
    /// A transmit error preserves the unsent response suffix.
    /// Receive errors are reported without implicitly clearing the parser.
    ///
    /// Byte budgets limit transport calls, not elapsed processing time.
    /// The master must wait for each response before sending another request.
    pub fn poll<T: ByteTransport>(
        &mut self,
        transport: &mut T,
        receive_budget: usize,
        transmit_budget: usize,
        checksum: &mut impl FnMut(&[u8]) -> u32,
    ) -> Result<FrameProgress, FrameProcessingError<T::Error>> {
        // Output servicing must remain reachable even during backpressure
        // or when no transport work is permitted.
        self.registers.service();

        let transmitted = self
            .response
            .service(transport, transmit_budget)
            .map_err(FrameProcessingError::Transmit)?;

        let mut progress = FrameProgress {
            transmitted,
            ..FrameProgress::default()
        };

        if self.response.is_pending() {
            return Ok(progress);
        }

        // The register response borrows payload storage until encoded.
        // queue() then copies the complete encoded response.
        let mut response_payload = [0; MAX_PAYLOAD_SIZE];
        let mut encoded = [0; MAX_FRAME_SIZE];

        loop {
            // Drain buffered complete requests before reading another byte
            // or applying an externally detected timeout.
            if let Some(request) = self.parser.next_frame(checksum) {
                let response = self
                    .registers
                    .handle(request, &mut response_payload)
                    .map_err(FrameProcessingError::Service)?;

                progress.processed += 1;

                if let Some(response) = response {
                    let length = encode_frame(
                        response.header,
                        response.payload,
                        FrameKind::Response,
                        &mut encoded,
                        checksum,
                    )
                    .map_err(FrameProcessingError::Encoding)?;

                    self.response
                        .queue(&encoded[..length])
                        .map_err(FrameProcessingError::Queue)?;

                    // Do not consume another request while its response
                    // cannot be stored. Transmission starts next poll.
                    return Ok(progress);
                }

                // Continue only if the service did not produce a response.
                continue;
            }

            if self.receive_timeout_pending {
                // No complete request remains buffered. Only the incomplete
                // remainder is discarded; pending responses were preserved.
                self.parser.on_timeout();
                self.receive_timeout_pending = false;
            }

            if progress.received >= receive_budget {
                return Ok(progress);
            }

            match transport
                .try_read()
                .map_err(FrameProcessingError::Receive)?
            {
                Some(byte) => {
                    self.parser
                        .push(byte)
                        .map_err(FrameProcessingError::Parser)?;

                    progress.received += 1;
                }
                None => {
                    progress.receive_empty = true;
                    return Ok(progress);
                }
            }
        }
    }

    /// Borrows a pending request once the software response buffer is empty.
    ///
    /// Transport acceptance does not guarantee delivery to the master.
    /// Repeated calls retain the request and Busy until completion.
    pub fn calibration_ready(&self) -> Option<&PendingCalibration<18>> {
        if self.response.is_pending() {
            return None;
        }

        self.registers.pending_calibration()
    }

    /// Reports completion of a previously released calibration request.
    ///
    /// The caller must finish persistence before reporting its result.
    pub fn complete_calibration(
        &mut self,
        result: CalibrationSaveResult,
    ) -> Result<(), CalibrationCompletionError> {
        self.registers
            .complete_calibration(result)
    }
}

#[cfg(test)]
#[path = "test_frame_processor.rs"]
mod tests;
