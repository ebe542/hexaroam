//! Read/write calibration storage for the separate station.

use embedded_hal::{delay::DelayNs, i2c::I2c};
use hexaroam_eeprom::Eeprom;
use hexaroam_servo_control::Calibration;
use hexaroam_servo_storage::{
    CalibrationLoadError, CalibrationRead, CalibrationSaveError,
    CalibrationStorage, Checksum, SlotLayoutError, load_calibrations,
    save_calibrations,
};

use crate::{EepromSlotError, EepromSlotWriter};

#[derive(Debug, PartialEq, Eq)]
pub enum EepromCalibrationStorageError<E> {
    Load(CalibrationLoadError<EepromSlotError<E>>),
    Save(CalibrationSaveError<EepromSlotError<E>>),
}

/// Owns the EEPROM slot writer, delay and checksum implementation.
///
/// Intended for the separate calibration station.
/// The operating controller uses EepromCalibrationReader instead.
pub struct EepromCalibrationStorage<
    I,
    D,
    C,
    const N: usize,
    const SLOTS: usize = 64,
> {
    writer: EepromSlotWriter<I, D, SLOTS>,
    checksum: C,
}

impl<I: I2c, D: DelayNs, C: Checksum, const N: usize, const SLOTS: usize>
    EepromCalibrationStorage<I, D, C, N, SLOTS>
{
    /// Checks the slot region without accessing the bus.
    pub fn new(
        eeprom: Eeprom<I>,
        delay: D,
        checksum: C,
    ) -> Result<Self, SlotLayoutError> {
        let writer = EepromSlotWriter::new(eeprom, delay)?;

        Ok(Self { writer, checksum })
    }
}

impl<I: I2c, D: DelayNs, C: Checksum, const N: usize, const SLOTS: usize>
    CalibrationRead<N> for EepromCalibrationStorage<I, D, C, N, SLOTS>
{
    type Error = EepromCalibrationStorageError<I::Error>;

    fn load(&mut self) -> Result<Option<[Calibration; N]>, Self::Error> {
        load_calibrations::<N, SLOTS, _>(&mut self.writer, &mut self.checksum)
            .map_err(EepromCalibrationStorageError::Load)
    }
}

impl<I: I2c, D: DelayNs, C: Checksum, const N: usize, const SLOTS: usize>
    CalibrationStorage<N> for EepromCalibrationStorage<I, D, C, N, SLOTS>
{
    fn save(
        &mut self,
        calibrations: &[Calibration; N],
    ) -> Result<(), Self::Error> {
        save_calibrations::<N, SLOTS, _>(
            &mut self.writer,
            calibrations,
            &mut self.checksum,
        )
        .map_err(EepromCalibrationStorageError::Save)
    }
}

#[cfg(test)]
#[path = "test_calibration_storage.rs"]
mod tests;
