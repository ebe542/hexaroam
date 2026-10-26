//! Bounded DMA abort for the polling-based signal driver.

use core::ptr::write_volatile;

use rp2040_hal::{
    dma::{Channel, ChannelIndex, ReadTarget, WriteTarget, single_buffer},
    pac,
};

// RP2040 peripheral atomic-clear alias, in bytes.
const ATOMIC_CLEAR_OFFSET: usize = 0x3000;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DmaAbortError {
    Timeout,
}

/// Aborts the owned transfer without releasing its resources.
///
/// Both interrupt routes are disabled for this channel and remain disabled.
/// This matches the signal driver's polling-only operation.
///
/// On timeout, the transfer and its buffer must remain owned by the caller.
/// The caller must not restart the channel or modify its source memory.
pub(crate) fn abort_transfer<CH, FROM, TO>(
    _transfer: &mut single_buffer::Transfer<Channel<CH>, FROM, TO>,
    poll_budget: usize,
) -> Result<(), DmaAbortError>
where
    CH: ChannelIndex,
    FROM: ReadTarget,
    TO: WriteTarget,
{
    let channel_index = usize::from(CH::id());
    let channel_mask = 1_u32 << CH::id();

    // SAFETY: the mutable transfer borrow represents ownership of this
    // channel and keeps its source and destination resources bound.
    // Other DMA channels are not modified.
    let registers = unsafe { &*pac::DMA::ptr() };

    // RP2040-E13: aborting can produce a completion interrupt.
    // Atomic aliases preserve the interrupt settings of other channels.
    for enable_register in
        [registers.inte0().as_ptr(), registers.inte1().as_ptr()]
    {
        // SAFETY: these are the documented RP2040 atomic-clear aliases.
        // The mask selects only the channel owned by this transfer.
        unsafe {
            write_volatile(
                enable_register.byte_add(ATOMIC_CLEAR_OFFSET),
                channel_mask,
            );
        }
    }

    registers.chan_abort().write(|w| {
        // SAFETY: ChannelIndex identifies an existing DMA channel.
        unsafe { w.bits(channel_mask) }
    });

    for _ in 0..poll_budget {
        let busy = registers
            .ch(channel_index)
            .ch_ctrl_trig()
            .read()
            .busy()
            .bit_is_set();

        let abort_pending =
            registers.chan_abort().read().bits() & channel_mask != 0;

        // RP2040-E13: checking only CHAN_ABORT is insufficient.
        // BUSY must also be clear before resources can be recovered.
        if !busy && !abort_pending {
            registers.intr().write(|w| {
                // SAFETY: INTR is write-one-to-clear.
                // Clear only this channel's pending interrupt.
                unsafe { w.bits(channel_mask) }
            });

            return Ok(());
        }
    }

    Err(DmaAbortError::Timeout)
}
