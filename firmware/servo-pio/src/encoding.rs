use hexaroam_servo_signal::{SignalPlan, SignalPlanEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EncodingError {
    InvalidGpioMask,
    UnconfiguredGpio,
    ZeroDuration,
    DurationOverflow,
    InsufficientCapacity,
}

/// Encodes one entry for the checked 25-MHz PIO program.
///
/// allowed_gpio_mask contains only configured servo outputs.
/// The result contains the marked GPIO mask followed by the counter.
pub(crate) fn encode_entry(
    entry: SignalPlanEntry,
    allowed_gpio_mask: u32,
) -> Result<[u32; 2], EncodingError> {
    // RP2040 exposes GPIO0 through GPIO29 in its normal GPIO bank.
    const SUPPORTED_GPIO_MASK: u32 = 0x3FFF_FFFF;
    const MASK_MARKER: u32 = 1_u32 << 31;

    // A 25 MHz PIO clock provides 25 cycles per microsecond.
    // For example, a 125 MHz system clock requires a divider of 5.
    const PIO_CYCLES_PER_US: u64 = 25;

    // Between consecutive OUT PINS updates, JMP Y-- takes counter + 1
    // cycles, followed by 10 instructions through the next OUT PINS.
    // Hardware wrap adds no cycles: total = counter + 1 + 10.
    // Therefore counter = duration_us * 25 - 11.
    // Must match pico/src/drivers/servo_plan_program.rs and its clock.
    const ENTRY_OVERHEAD_CYCLES: u64 = 11;

    if allowed_gpio_mask == 0 || allowed_gpio_mask & !SUPPORTED_GPIO_MASK != 0 {
        return Err(EncodingError::InvalidGpioMask);
    }

    if entry.gpio_mask & !allowed_gpio_mask != 0 {
        return Err(EncodingError::UnconfiguredGpio);
    }

    if entry.duration_us == 0 {
        return Err(EncodingError::ZeroDuration);
    }

    let counter = u64::from(entry.duration_us) * PIO_CYCLES_PER_US
        - ENTRY_OVERHEAD_CYCLES;

    let counter =
        u32::try_from(counter).map_err(|_| EncodingError::DurationOverflow)?;

    Ok([entry.gpio_mask | MASK_MARKER, counter])
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EncodedSignalPlan<const WORD_CAPACITY: usize> {
    words: [u32; WORD_CAPACITY],
    len: usize,
}

impl<const WORD_CAPACITY: usize> EncodedSignalPlan<WORD_CAPACITY> {
    pub(crate) fn new<const PLAN_CAPACITY: usize>(
        plan: &SignalPlan<PLAN_CAPACITY>,
        allowed_gpio_mask: u32,
    ) -> Result<Self, EncodingError> {
        let entries = plan.entries();

        // Each entry requires two words. Division avoids multiplication overflow.
        if entries.len() > WORD_CAPACITY / 2 {
            return Err(EncodingError::InsufficientCapacity);
        }

        let mut encoded = Self {
            words: [0; WORD_CAPACITY],
            len: 0,
        };

        for &entry in entries {
            let words = encode_entry(entry, allowed_gpio_mask)?;

            encoded.words[encoded.len..encoded.len + 2].copy_from_slice(&words);
            encoded.len += 2;
        }

        Ok(encoded)
    }

    pub(crate) fn words(&self) -> &[u32] {
        &self.words[..self.len]
    }
}

#[cfg(test)]
#[path = "test_encoding.rs"]
mod tests;
