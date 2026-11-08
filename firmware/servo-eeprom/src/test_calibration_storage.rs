extern crate std;

use std::vec::Vec;

use embedded_hal::{
    delay::DelayNs,
    i2c::{ErrorKind, ErrorType, I2c, Operation},
};
use hexaroam_eeprom::{CAPACITY, Eeprom, EepromError};
use hexaroam_servo_control::{Angle, Calibration};
use hexaroam_servo_storage::{
    CalibrationLoadError, CalibrationRead, CalibrationSaveError,
    CalibrationStorage, CalibrationWritePlanError, SLOT_SIZE, SlotLayoutError,
    SlotWriteError, SoftwareCrc32, decode_slot, encode_slot,
};

use super::{EepromCalibrationStorage, EepromCalibrationStorageError};
use crate::EepromSlotError;

type StoreError = EepromCalibrationStorageError<ErrorKind>;

struct TestBus {
    bytes: [u8; CAPACITY],
    reads: Vec<u16>,
    programs: Vec<u16>,
    fail_read_number: Option<usize>,
    fail_write_number: Option<usize>,
    write_protected: bool,
}

impl TestBus {
    fn new() -> Self {
        Self {
            bytes: [0xFF; CAPACITY],
            reads: Vec::new(),
            programs: Vec::new(),
            fail_read_number: None,
            fail_write_number: None,
            write_protected: false,
        }
    }

    fn seed(
        &mut self,
        slot_index: usize,
        sequence: u64,
        value: Option<Calibration>,
    ) {
        let mut slot = [0xFF; SLOT_SIZE];

        encode_slot(
            sequence,
            &[value],
            &mut slot,
            &mut SoftwareCrc32,
        )
        .unwrap();

        let start = slot_index * SLOT_SIZE;
        self.bytes[start..start + SLOT_SIZE].copy_from_slice(&slot);
    }

    fn slot(&self, index: usize) -> [u8; SLOT_SIZE] {
        let start = index * SLOT_SIZE;
        self.bytes[start..start + SLOT_SIZE]
            .try_into()
            .unwrap()
    }
}

impl ErrorType for TestBus {
    type Error = ErrorKind;
}

impl I2c for TestBus {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        assert_eq!(address, 0x53);

        match operations {
            [Operation::Write(bytes)] => {
                assert!(bytes.len() >= 2);

                let offset = u16::from_be_bytes([bytes[0], bytes[1]]);
                let start = usize::from(offset);
                let data = &bytes[2..];

                // Pointer-only probes acknowledge immediately.
                if data.is_empty() {
                    return Ok(());
                }

                assert!(data.len() <= 64);
                assert!(start % 64 + data.len() <= 64);
                assert!(start + data.len() <= CAPACITY);

                self.programs.push(offset);

                if self.fail_write_number == Some(self.programs.len()) {
                    self.bytes[start] = data[0];
                    return Err(ErrorKind::Bus);
                }

                // WP acknowledges writes without changing memory.
                if !self.write_protected {
                    self.bytes[start..start + data.len()].copy_from_slice(data);
                }

                Ok(())
            }

            [Operation::Write(pointer), Operation::Read(output)] => {
                assert_eq!(pointer.len(), 2);

                let offset = u16::from_be_bytes([pointer[0], pointer[1]]);
                let start = usize::from(offset);

                self.reads.push(offset);

                if self.fail_read_number == Some(self.reads.len()) {
                    return Err(ErrorKind::Bus);
                }

                assert!(start + output.len() <= CAPACITY);
                output
                    .copy_from_slice(&self.bytes[start..start + output.len()]);

                Ok(())
            }

            _ => panic!("Unexpected I2C transaction"),
        }
    }
}

struct NoDelay;

impl DelayNs for NoDelay {
    fn delay_ns(&mut self, _ns: u32) {
        panic!("The test EEPROM acknowledges readiness immediately");
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

fn store<const N: usize, const SLOTS: usize>(
    bus: &mut TestBus,
) -> EepromCalibrationStorage<&mut TestBus, NoDelay, SoftwareCrc32, N, SLOTS> {
    EepromCalibrationStorage::new(
        Eeprom::new(bus, 0x53).unwrap(),
        NoDelay,
        SoftwareCrc32,
    )
    .unwrap()
}

fn construct<const SLOTS: usize>(
    bus: &mut TestBus,
) -> Result<(), SlotLayoutError> {
    EepromCalibrationStorage::<_, _, _, 1, SLOTS>::new(
        Eeprom::new(bus, 0x53).unwrap(),
        NoDelay,
        SoftwareCrc32,
    )
    .map(|_| ())
}

#[test]
fn construction_performs_no_bus_access() {
    let mut bus = TestBus::new();

    assert_eq!(construct::<64>(&mut bus), Ok(()));
    assert!(bus.reads.is_empty());
    assert!(bus.programs.is_empty());
}

#[test]
fn invalid_regions_are_rejected_without_bus_access() {
    let mut bus = TestBus::new();

    assert_eq!(
        construct::<0>(&mut bus),
        Err(SlotLayoutError::EmptyRegion)
    );
    assert_eq!(
        construct::<65>(&mut bus),
        Err(SlotLayoutError::RegionOutOfBounds)
    );
    assert!(bus.reads.is_empty());
    assert!(bus.programs.is_empty());
}

#[test]
fn erased_storage_loads_as_none_without_programming() {
    let mut bus = TestBus::new();

    assert_eq!(store::<1, 4>(&mut bus).load(), Ok(None));
    assert_eq!(
        bus.reads.as_slice(),
        &[0, 512, 1_024, 1_536]
    );
    assert!(bus.programs.is_empty());
}

#[test]
fn saved_dataset_can_be_loaded_through_the_same_adapter() {
    let mut bus = TestBus::new();
    let values = [calibration(1_100)];
    let mut storage = store::<1, 4>(&mut bus);

    assert_eq!(storage.save(&values), Ok(()));
    assert_eq!(storage.load(), Ok(Some(values)));
}

#[test]
fn saved_dataset_survives_reconstructing_the_adapter() {
    let mut bus = TestBus::new();
    let values = [calibration(1_100)];

    {
        let mut storage = store::<1, 4>(&mut bus);
        storage.save(&values).unwrap();
    }

    assert_eq!(
        store::<1, 4>(&mut bus).load(),
        Ok(Some(values))
    );
}

#[test]
fn repeated_identical_save_does_not_program_again() {
    let mut bus = TestBus::new();
    let values = [calibration(1_100)];

    store::<1, 4>(&mut bus)
        .save(&values)
        .unwrap();
    let programs = bus.programs.len();
    assert!(programs > 0);

    store::<1, 4>(&mut bus)
        .save(&values)
        .unwrap();

    assert_eq!(bus.programs.len(), programs);
}

#[test]
fn changed_dataset_rotates_and_preserves_original_slot() {
    let mut bus = TestBus::new();
    bus.seed(0, 10, Some(calibration(1_000)));
    let original = bus.slot(0);
    let values = [calibration(1_100)];

    store::<1, 4>(&mut bus)
        .save(&values)
        .unwrap();

    assert_eq!(bus.slot(0), original);
    assert!(
        bus.programs
            .iter()
            .all(|&offset| { (512..1_024).contains(&usize::from(offset)) })
    );

    let record = decode_slot::<1>(&bus.slot(1), &mut SoftwareCrc32).unwrap();

    assert_eq!(record.sequence, 11);
    assert_eq!(record.calibrations, values.map(Some));
}

#[test]
fn rotation_wraps_to_first_slot() {
    let mut bus = TestBus::new();
    bus.seed(3, 10, Some(calibration(1_000)));
    let original = bus.slot(3);

    store::<1, 4>(&mut bus)
        .save(&[calibration(1_100)])
        .unwrap();

    let record = decode_slot::<1>(&bus.slot(0), &mut SoftwareCrc32).unwrap();

    assert_eq!(record.sequence, 11);
    assert_eq!(bus.slot(3), original);
    assert!(
        bus.programs
            .iter()
            .all(|&offset| usize::from(offset) < 512)
    );
}

#[test]
fn incomplete_dataset_is_reported_as_load_error() {
    let mut bus = TestBus::new();
    bus.seed(0, 10, None);

    assert_eq!(
        store::<1, 4>(&mut bus).load(),
        Err(StoreError::Load(
            CalibrationLoadError::IncompleteCalibration(0)
        ))
    );
}

#[test]
fn incomplete_dataset_can_be_replaced_with_complete_data() {
    let mut bus = TestBus::new();
    bus.seed(0, 10, None);
    let values = [calibration(1_100)];
    let mut storage = store::<1, 4>(&mut bus);

    assert_eq!(storage.save(&values), Ok(()));
    assert_eq!(storage.load(), Ok(Some(values)));
}

#[test]
fn load_preserves_underlying_bus_error() {
    let mut bus = TestBus::new();
    bus.fail_read_number = Some(1);

    assert_eq!(
        store::<1, 4>(&mut bus).load(),
        Err(StoreError::Load(
            CalibrationLoadError::Storage(EepromSlotError::Device(
                EepromError::Bus(ErrorKind::Bus)
            ))
        ))
    );
    assert!(bus.programs.is_empty());
}

#[test]
fn save_scan_error_prevents_programming() {
    let mut bus = TestBus::new();
    bus.fail_read_number = Some(1);

    assert_eq!(
        store::<1, 4>(&mut bus).save(&[calibration(1_100)]),
        Err(StoreError::Save(
            CalibrationSaveError::Scan(CalibrationLoadError::Storage(
                EepromSlotError::Device(EepromError::Bus(ErrorKind::Bus))
            ))
        ))
    );
    assert!(bus.programs.is_empty());
}

#[test]
fn programming_error_is_preserved_without_retry() {
    let mut bus = TestBus::new();
    bus.fail_write_number = Some(1);

    assert_eq!(
        store::<1, 4>(&mut bus).save(&[calibration(1_100)]),
        Err(StoreError::Save(
            CalibrationSaveError::Write(SlotWriteError::Storage(
                EepromSlotError::Device(EepromError::Bus(ErrorKind::Bus))
            ))
        ))
    );
    assert_eq!(bus.programs.len(), 1);
}

#[test]
fn write_protection_cannot_be_reported_as_success() {
    let mut bus = TestBus::new();
    bus.write_protected = true;

    assert_eq!(
        store::<1, 4>(&mut bus).save(&[calibration(1_100)]),
        Err(StoreError::Save(
            CalibrationSaveError::AttemptsExhausted {
                attempts: 3,
                last_error: SlotWriteError::InvalidationFailed,
            }
        ))
    );
    assert_eq!(
        bus.programs.as_slice(),
        &[0, 512, 1_024]
    );
    assert!(
        bus.bytes
            .iter()
            .all(|&byte| byte == 0xFF)
    );
}

#[test]
fn zero_servos_are_rejected_without_bus_access() {
    let mut bus = TestBus::new();
    let mut storage = store::<0, 4>(&mut bus);

    assert_eq!(
        storage.load(),
        Err(StoreError::Load(
            CalibrationLoadError::InvalidServoCount
        ))
    );
    assert_eq!(
        storage.save(&[]),
        Err(StoreError::Save(
            CalibrationSaveError::Plan(
                CalibrationWritePlanError::InvalidServoCount
            )
        ))
    );

    assert!(bus.reads.is_empty());
    assert!(bus.programs.is_empty());
}

#[test]
fn duplicate_sequences_prevent_loading_and_saving() {
    let mut bus = TestBus::new();
    bus.seed(0, 10, Some(calibration(1_000)));
    bus.seed(3, 10, Some(calibration(1_000)));
    let mut storage = store::<1, 4>(&mut bus);

    assert_eq!(
        storage.load(),
        Err(StoreError::Load(
            CalibrationLoadError::DuplicateSequence(10)
        ))
    );
    assert_eq!(
        storage.save(&[calibration(1_100)]),
        Err(StoreError::Save(
            CalibrationSaveError::Scan(
                CalibrationLoadError::DuplicateSequence(10)
            )
        ))
    );

    assert!(bus.programs.is_empty());
}
