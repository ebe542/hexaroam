#![no_std]

//! Read-only EEPROM adapter for complete servo calibration.

use embedded_hal::i2c::I2c;
use hexaroam_eeprom::{CAPACITY, Eeprom, EepromError};
use hexaroam_servo_control::Calibration;
use hexaroam_servo_storage::{
    CalibrationLoadError, CalibrationRead, Checksum, JournalRead, SLOT_SIZE,
    SlotLayout, SlotLayoutError, load_calibrations,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EepromSlotError<E> {
    Layout(SlotLayoutError),
    Device(EepromError<E>),
}

struct EepromSlots<I> {
    eeprom: Eeprom<I>,
    layout: SlotLayout,
}

impl<I: I2c> JournalRead for EepromSlots<I> {
    type Error = EepromSlotError<I::Error>;

    fn read_slot(
        &mut self,
        slot_index: usize,
        output: &mut [u8; SLOT_SIZE],
    ) -> Result<(), Self::Error> {
        let range = self
            .layout
            .slot_range(slot_index)
            .map_err(EepromSlotError::Layout)?;

        // Construction validated the region against the 32 KiB device.
        self.eeprom
            .read(range.offset as u16, output)
            .map_err(EepromSlotError::Device)
    }
}

/// Owns the EEPROM and checksum implementation.
///
/// Slots begin at address zero. The configured region must fit
/// within the EEPROM. Construction performs no bus access.
/// The default slot count uses the complete 32 KiB device.
pub struct EepromCalibrationReader<
    I,
    C,
    const N: usize,
    const SLOTS: usize = 64,
> {
    slots: EepromSlots<I>,
    checksum: C,
}

impl<I: I2c, C: Checksum, const N: usize, const SLOTS: usize>
    EepromCalibrationReader<I, C, N, SLOTS>
{
    pub fn new(
        eeprom: Eeprom<I>,
        checksum: C,
    ) -> Result<Self, SlotLayoutError> {
        let layout = SlotLayout::new(0, SLOTS, CAPACITY as u32)?;

        Ok(Self {
            slots: EepromSlots { eeprom, layout },
            checksum,
        })
    }
}

impl<I: I2c, C: Checksum, const N: usize, const SLOTS: usize> CalibrationRead<N>
    for EepromCalibrationReader<I, C, N, SLOTS>
{
    type Error = CalibrationLoadError<EepromSlotError<I::Error>>;

    fn load(&mut self) -> Result<Option<[Calibration; N]>, Self::Error> {
        load_calibrations::<N, SLOTS, _>(&mut self.slots, &mut self.checksum)
    }
}

#[cfg(test)]
#[path = "test_reader.rs"]
mod tests;
