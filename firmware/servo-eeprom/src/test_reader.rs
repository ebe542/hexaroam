extern crate std;

use std::vec::Vec;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};
use hexaroam_eeprom::{Eeprom, EepromError};
use hexaroam_servo_control::{Angle, Calibration};
use hexaroam_servo_storage::{
    CalibrationLoadError, CalibrationRead, JournalRead, SLOT_SIZE,
    SlotLayoutError, SoftwareCrc32, encode_slot,
};

use super::{EepromCalibrationReader, EepromSlotError};

struct TestBus<const SLOTS: usize> {
    slots: [[u8; SLOT_SIZE]; SLOTS],
    offsets: Vec<u16>,
    fail_at: Option<usize>,
}

impl<const SLOTS: usize> TestBus<SLOTS> {
    fn new() -> Self {
        Self {
            slots: [[0xFF; SLOT_SIZE]; SLOTS],
            offsets: Vec::new(),
            fail_at: None,
        }
    }

    fn seed<const N: usize>(
        &mut self,
        index: usize,
        sequence: u64,
        calibrations: [Option<Calibration>; N],
    ) {
        encode_slot(
            sequence,
            &calibrations,
            &mut self.slots[index],
            &mut SoftwareCrc32,
        )
        .unwrap();
    }
}

impl<const SLOTS: usize> ErrorType for TestBus<SLOTS> {
    type Error = ErrorKind;
}

impl<const SLOTS: usize> I2c for TestBus<SLOTS> {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        assert_eq!(address, 0x53);

        let [Operation::Write(pointer), Operation::Read(output)] = operations
        else {
            panic!("Expected a combined address-write/read");
        };

        assert_eq!(pointer.len(), 2);
        assert_eq!(output.len(), SLOT_SIZE);

        let offset = u16::from_be_bytes([pointer[0], pointer[1]]);
        assert_eq!(usize::from(offset) % SLOT_SIZE, 0);

        self.offsets.push(offset);
        let index = usize::from(offset) / SLOT_SIZE;

        if self.fail_at == Some(index) {
            return Err(ErrorKind::Bus);
        }

        output.copy_from_slice(&self.slots[index]);
        Ok(())
    }
}

fn calibration(lower_pulse: u32) -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(-4_500),
        lower_pulse,
        Angle::from_centidegrees(4_500),
        lower_pulse + 1_000,
    )
    .unwrap()
}

fn complete() -> [Option<Calibration>; 2] {
    [Some(calibration(1_000)), Some(calibration(1_200))]
}

fn expected() -> [Calibration; 2] {
    [calibration(1_000), calibration(1_200)]
}

fn reader<const N: usize, const SLOTS: usize>(
    bus: &mut TestBus<SLOTS>,
) -> EepromCalibrationReader<&mut TestBus<SLOTS>, SoftwareCrc32, N, SLOTS> {
    let eeprom = Eeprom::new(bus, 0x53).unwrap();

    EepromCalibrationReader::new(eeprom, SoftwareCrc32).unwrap()
}

#[test]
fn construction_does_not_access_bus() {
    let mut bus = TestBus::<64>::new();

    {
        let _reader = reader::<2, 64>(&mut bus);
    }

    assert!(bus.offsets.is_empty());
}

#[test]
fn zero_slots_are_rejected_without_bus_access() {
    let mut bus = TestBus::<0>::new();
    let eeprom = Eeprom::new(&mut bus, 0x53).unwrap();

    let result =
        EepromCalibrationReader::<_, _, 2, 0>::new(eeprom, SoftwareCrc32);

    assert!(matches!(
        result,
        Err(SlotLayoutError::EmptyRegion)
    ));
    assert!(bus.offsets.is_empty());
}

#[test]
fn region_exceeding_eeprom_is_rejected_without_bus_access() {
    let mut bus = TestBus::<1>::new();
    let eeprom = Eeprom::new(&mut bus, 0x53).unwrap();

    let result =
        EepromCalibrationReader::<_, _, 2, 65>::new(eeprom, SoftwareCrc32);

    assert!(matches!(
        result,
        Err(SlotLayoutError::RegionOutOfBounds)
    ));
    assert!(bus.offsets.is_empty());
}

#[test]
fn slot_index_maps_to_its_byte_address() {
    let mut bus = TestBus::<4>::new();
    bus.slots[2] = [0xAB; SLOT_SIZE];
    let mut output = [0; SLOT_SIZE];

    reader::<2, 4>(&mut bus)
        .slots
        .read_slot(2, &mut output)
        .unwrap();

    assert_eq!(output, [0xAB; SLOT_SIZE]);
    assert_eq!(bus.offsets.as_slice(), &[1_024]);
}

#[test]
fn last_slot_maps_to_last_512_bytes() {
    let mut bus = TestBus::<64>::new();
    bus.slots[63] = [0xCD; SLOT_SIZE];
    let mut output = [0; SLOT_SIZE];

    reader::<2, 64>(&mut bus)
        .slots
        .read_slot(63, &mut output)
        .unwrap();

    assert_eq!(output, [0xCD; SLOT_SIZE]);
    assert_eq!(bus.offsets.as_slice(), &[32_256]);
}

#[test]
fn invalid_slot_is_rejected_without_bus_access() {
    let mut bus = TestBus::<4>::new();
    let mut output = [0xAA; SLOT_SIZE];

    let result = reader::<2, 4>(&mut bus)
        .slots
        .read_slot(4, &mut output);

    assert_eq!(
        result,
        Err(EepromSlotError::Layout(
            SlotLayoutError::UnknownSlot(4)
        ))
    );
    assert_eq!(output, [0xAA; SLOT_SIZE]);
    assert!(bus.offsets.is_empty());
}

#[test]
fn erased_eeprom_returns_none_after_reading_every_slot() {
    let mut bus = TestBus::<64>::new();

    assert_eq!(
        reader::<2, 64>(&mut bus).load(),
        Ok(None)
    );

    let expected: Vec<u16> = (0..64)
        .map(|index| index * 512)
        .collect();

    assert_eq!(bus.offsets, expected);
}

#[test]
fn complete_dataset_loads_through_eeprom_and_storage_layers() {
    let mut bus = TestBus::<4>::new();
    bus.seed(2, 10, complete());

    assert_eq!(
        reader::<2, 4>(&mut bus).load(),
        Ok(Some(expected()))
    );
    assert_eq!(
        bus.offsets.as_slice(),
        &[0, 512, 1_024, 1_536]
    );
}

#[test]
fn newest_dataset_is_selected_independently_of_slot_position() {
    let mut bus = TestBus::<4>::new();
    bus.seed(0, 20, complete());
    bus.seed(3, 10, [Some(calibration(800)); 2]);

    assert_eq!(
        reader::<2, 4>(&mut bus).load(),
        Ok(Some(expected()))
    );
}

#[test]
fn corrupt_newer_dataset_is_skipped() {
    let mut bus = TestBus::<4>::new();
    bus.seed(0, 10, complete());
    bus.seed(3, 20, [Some(calibration(800)); 2]);
    bus.slots[3][4] ^= 1;

    assert_eq!(
        reader::<2, 4>(&mut bus).load(),
        Ok(Some(expected()))
    );
}

#[test]
fn incomplete_newest_dataset_prevents_fallback() {
    let mut bus = TestBus::<4>::new();
    bus.seed(0, 10, complete());
    bus.seed(3, 20, [Some(calibration(800)), None]);

    assert_eq!(
        reader::<2, 4>(&mut bus).load(),
        Err(CalibrationLoadError::IncompleteCalibration(1))
    );
}

#[test]
fn duplicate_sequence_is_forwarded() {
    let mut bus = TestBus::<4>::new();
    bus.seed(0, 20, complete());
    bus.seed(3, 20, complete());

    assert_eq!(
        reader::<2, 4>(&mut bus).load(),
        Err(CalibrationLoadError::DuplicateSequence(
            20
        ))
    );
}

#[test]
fn slot_read_preserves_original_bus_error() {
    let mut bus = TestBus::<4>::new();
    bus.fail_at = Some(2);
    let mut output = [0; SLOT_SIZE];

    assert_eq!(
        reader::<2, 4>(&mut bus)
            .slots
            .read_slot(2, &mut output),
        Err(EepromSlotError::Device(
            EepromError::Bus(ErrorKind::Bus)
        ))
    );
    assert_eq!(bus.offsets.as_slice(), &[1_024]);
}

#[test]
fn later_bus_error_prevents_returning_an_earlier_dataset() {
    let mut bus = TestBus::<4>::new();
    bus.seed(0, 10, complete());
    bus.fail_at = Some(3);

    assert_eq!(
        reader::<2, 4>(&mut bus).load(),
        Err(CalibrationLoadError::Storage(
            EepromSlotError::Device(EepromError::Bus(ErrorKind::Bus))
        ))
    );
    assert_eq!(
        bus.offsets.as_slice(),
        &[0, 512, 1_024, 1_536]
    );
}

#[test]
fn zero_servos_are_rejected_before_bus_access() {
    let mut bus = TestBus::<4>::new();

    assert_eq!(
        reader::<0, 4>(&mut bus).load(),
        Err(CalibrationLoadError::InvalidServoCount)
    );
    assert!(bus.offsets.is_empty());
}
