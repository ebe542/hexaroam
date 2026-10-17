//! Exclusive DMA ownership of encoded signal-plan storage.

use core::ops::Range;

use rp2040_hal::dma::ReadTarget;

use crate::OutputBuffer;

pub(crate) struct SignalPlanDmaSource<const WORD_CAPACITY: usize> {
    buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
    range: Range<usize>,
}

impl<const WORD_CAPACITY: usize> SignalPlanDmaSource<WORD_CAPACITY> {
    pub(crate) fn new(
        buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
        range: Range<usize>,
    ) -> Self {
        assert!(
            range.start < range.end && range.end <= buffer.words().len(),
            "DMA range must select existing signal-plan words",
        );

        Self { buffer, range }
    }

    pub(crate) fn into_buffer(
        self,
    ) -> &'static mut OutputBuffer<WORD_CAPACITY> {
        self.buffer
    }
}

// SAFETY:
// - The exclusive reference points to statically allocated storage.
// - Moving this source does not move the referenced storage.
// - The selected range is validated at construction.
// - No method changes the payload or range while this source exists.
// - The HAL owns this source throughout the DMA transfer.
// - The buffer can be recovered only by consuming the source,
//   which the driver does after the transfer has completed.
// - This module is compiled only for the 32-bit RP2040 target.
unsafe impl<const WORD_CAPACITY: usize> ReadTarget
    for SignalPlanDmaSource<WORD_CAPACITY>
{
    type ReceivedWord = u32;

    fn rx_treq() -> Option<u8> {
        // RAM is not paced; the destination PIO FIFO supplies DREQ.
        None
    }

    fn rx_address_count(&self) -> (u32, u32) {
        let words = &self.buffer.words()[self.range.clone()];

        (
            words.as_ptr() as u32,
            words.len() as u32,
        )
    }

    fn rx_increment(&self) -> bool {
        true
    }
}
