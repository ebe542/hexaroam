//! EEPROM slot writes for the separate calibration station.

use embedded_hal::{delay::DelayNs, i2c::I2c};
use hexaroam_eeprom::{CAPACITY, Eeprom};
use hexaroam_servo_storage::{
    JournalRead, SLOT_SIZE, SlotLayout, SlotLayoutError, SlotWrite,
};

use crate::{EepromSlotError, EepromSlots};

pub struct EepromSlotWriter<I, D, const SLOTS: usize = 64> {
    slots: EepromSlots<I>,
    delay: D,
}

impl<I: I2c, D: DelayNs, const SLOTS: usize> EepromSlotWriter<I, D, SLOTS> {
    /// Creates a checked slot region starting at zero.
    /// Construction performs no bus access or delay.
    pub fn new(eeprom: Eeprom<I>, delay: D) -> Result<Self, SlotLayoutError> {
        let layout = SlotLayout::new(0, SLOTS, CAPACITY as u32)?;

        Ok(Self {
            slots: EepromSlots { eeprom, layout },
            delay,
        })
    }
}

impl<I: I2c, D: DelayNs, const SLOTS: usize> JournalRead
    for EepromSlotWriter<I, D, SLOTS>
{
    type Error = EepromSlotError<I::Error>;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        self.slots.read_slot(slot_index, output)
    }
}

impl<I: I2c, D: DelayNs, const SLOTS: usize> SlotWrite
    for EepromSlotWriter<I, D, SLOTS>
{
    /// Rejects invalid slots and ranges before accessing the bus.
    ///
    /// An empty write is allowed at offsets 0..=SLOT_SIZE.
    /// Writes must never cross the logical slot boundary.
    fn write_slot_bytes(
        &mut self,
        slot_index: usize,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        let range = self
            .slots
            .layout
            .slot_range(slot_index)
            .map_err(EepromSlotError::Layout)?;

        if offset > range.length || data.len() > range.length - offset {
            return Err(EepromSlotError::RangeOutOfBounds);
        }

        if data.is_empty() {
            return Ok(());
        }

        // Layout and range validation guarantee that this nonempty
        // write starts inside the 32 KiB EEPROM.
        let address = (range.offset + offset as u32) as u16;

        self.slots
            .eeprom
            .write(address, data, &mut self.delay)
            .map_err(EepromSlotError::Device)
    }
}

#[cfg(test)]
#[path = "test_slot_writer.rs"]
mod tests;
