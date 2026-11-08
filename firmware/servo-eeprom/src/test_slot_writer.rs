extern crate std;

use std::{cell::RefCell, collections::VecDeque, rc::Rc, vec, vec::Vec};

use embedded_hal::{
    delay::DelayNs,
    i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation},
};
use hexaroam_eeprom::{Eeprom, EepromError};
use hexaroam_servo_storage::{
    JournalRead, SLOT_SIZE, SlotLayoutError, SlotWrite,
};

use super::EepromSlotWriter;
use crate::EepromSlotError;

const ADDRESS_NACK: ErrorKind =
    ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address);

enum Step {
    Write(Vec<u8>, Result<(), ErrorKind>),
    Read(
        u16,
        [u8; SLOT_SIZE],
        Result<(), ErrorKind>,
    ),
    Delay(u32),
}

type Script = Rc<RefCell<VecDeque<Step>>>;

struct TestBus {
    script: Script,
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

        let step = self
            .script
            .borrow_mut()
            .pop_front()
            .expect("Unexpected bus access");

        match step {
            Step::Write(expected, result) => {
                let [Operation::Write(actual)] = operations else {
                    panic!("Expected one write operation");
                };

                assert_eq!(*actual, expected.as_slice());
                result
            }
            Step::Read(offset, data, result) => {
                let [Operation::Write(pointer), Operation::Read(output)] =
                    operations
                else {
                    panic!("Expected a combined address-write/read");
                };

                assert_eq!(*pointer, &offset.to_be_bytes());
                assert_eq!(output.len(), SLOT_SIZE);
                output.copy_from_slice(&data);
                result
            }
            Step::Delay(_) => {
                panic!("Expected a delay before bus access");
            }
        }
    }
}

struct TestDelay {
    script: Script,
}

impl DelayNs for TestDelay {
    fn delay_ns(&mut self, ns: u32) {
        let step = self
            .script
            .borrow_mut()
            .pop_front()
            .expect("Unexpected delay");

        match step {
            Step::Delay(expected) => assert_eq!(ns, expected),
            _ => panic!("Expected bus access before delay"),
        }
    }
}

fn setup(steps: Vec<Step>) -> (TestBus, TestDelay, Script) {
    let script = Rc::new(RefCell::new(VecDeque::from(steps)));

    (
        TestBus {
            script: Rc::clone(&script),
        },
        TestDelay {
            script: Rc::clone(&script),
        },
        script,
    )
}

fn write(offset: u16, data: &[u8], result: Result<(), ErrorKind>) -> Step {
    let mut bytes = offset.to_be_bytes().to_vec();
    bytes.extend_from_slice(data);
    Step::Write(bytes, result)
}

fn poll(offset: u16, result: Result<(), ErrorKind>) -> Step {
    write(offset, &[], result)
}

fn construct<const SLOTS: usize>() -> Result<(), SlotLayoutError> {
    let (bus, delay, script) = setup(vec![]);
    let eeprom = Eeprom::new(bus, 0x53).unwrap();

    let result =
        EepromSlotWriter::<_, _, SLOTS>::new(eeprom, delay).map(|_| ());

    assert!(script.borrow().is_empty());
    result
}

fn run(
    slot_index: usize,
    offset: usize,
    data: &[u8],
    steps: Vec<Step>,
) -> Result<(), EepromSlotError<ErrorKind>> {
    let (bus, delay, script) = setup(steps);
    let eeprom = Eeprom::new(bus, 0x53).unwrap();
    let mut writer = EepromSlotWriter::<_, _, 64>::new(eeprom, delay).unwrap();

    let result = writer.write_slot_bytes(slot_index, offset, data);

    assert!(
        script.borrow().is_empty(),
        "Missing expected operations"
    );
    result
}

#[test]
fn construction_performs_no_bus_access_or_delay() {
    assert_eq!(construct::<64>(), Ok(()));
}

#[test]
fn invalid_slot_regions_are_rejected() {
    assert_eq!(
        construct::<0>(),
        Err(SlotLayoutError::EmptyRegion)
    );
    assert_eq!(
        construct::<65>(),
        Err(SlotLayoutError::RegionOutOfBounds)
    );
}

#[test]
fn slot_read_delegates_to_existing_reader() {
    let (bus, delay, script) = setup(vec![Step::Read(
        1_024,
        [0xAB; SLOT_SIZE],
        Ok(()),
    )]);

    let eeprom = Eeprom::new(bus, 0x53).unwrap();
    let mut writer = EepromSlotWriter::<_, _, 64>::new(eeprom, delay).unwrap();
    let mut output = [0; SLOT_SIZE];

    writer
        .read_slot(2, &mut output)
        .unwrap();

    assert_eq!(output, [0xAB; SLOT_SIZE]);
    assert!(script.borrow().is_empty());
}

#[test]
fn slot_read_preserves_bus_error() {
    let (bus, delay, script) = setup(vec![Step::Read(
        1_024,
        [0; SLOT_SIZE],
        Err(ErrorKind::Bus),
    )]);

    let eeprom = Eeprom::new(bus, 0x53).unwrap();
    let mut writer = EepromSlotWriter::<_, _, 64>::new(eeprom, delay).unwrap();

    assert_eq!(
        writer.read_slot(2, &mut [0; SLOT_SIZE]),
        Err(EepromSlotError::Device(
            EepromError::Bus(ErrorKind::Bus)
        ))
    );
    assert!(script.borrow().is_empty());
}

#[test]
fn writes_at_start_of_first_slot() {
    assert_eq!(
        run(
            0,
            0,
            &[0xAB],
            vec![write(0, &[0xAB], Ok(())), poll(0, Ok(())),],
        ),
        Ok(())
    );
}

#[test]
fn combines_slot_address_with_relative_offset() {
    assert_eq!(
        run(
            2,
            10,
            &[0xAB, 0xCD],
            vec![write(1_034, &[0xAB, 0xCD], Ok(())), poll(1_034, Ok(())),],
        ),
        Ok(())
    );
}

#[test]
fn delegates_physical_page_splitting_to_eeprom_driver() {
    assert_eq!(
        run(
            2,
            63,
            &[0x11, 0x22],
            vec![
                write(1_087, &[0x11], Ok(())),
                poll(1_087, Ok(())),
                write(1_088, &[0x22], Ok(())),
                poll(1_088, Ok(())),
            ],
        ),
        Ok(())
    );
}

#[test]
fn writes_can_end_exactly_at_the_last_slot_boundary() {
    assert_eq!(
        run(
            63,
            510,
            &[0xAB, 0xCD],
            vec![write(32_766, &[0xAB, 0xCD], Ok(())), poll(32_766, Ok(())),],
        ),
        Ok(())
    );
}

#[test]
fn invalid_ranges_are_rejected_before_any_write() {
    let data = [0xAA; 65];

    for (offset, length) in
        [(448, 65), (511, 2), (512, 1), (513, 0), (usize::MAX, 0)]
    {
        assert_eq!(
            run(2, offset, &data[..length], vec![]),
            Err(EepromSlotError::RangeOutOfBounds)
        );
    }
}

#[test]
fn empty_valid_writes_perform_no_bus_access_or_delay() {
    for slot_index in [0, 63] {
        for offset in [0, 63, 511, 512] {
            assert_eq!(
                run(slot_index, offset, &[], vec![]),
                Ok(())
            );
        }
    }
}

#[test]
fn invalid_slots_are_rejected_even_for_empty_writes() {
    for slot_index in [64, usize::MAX] {
        assert_eq!(
            run(slot_index, 0, &[], vec![]),
            Err(EepromSlotError::Layout(
                SlotLayoutError::UnknownSlot(slot_index)
            ))
        );
    }
}

#[test]
fn page_write_error_is_preserved_without_retry() {
    assert_eq!(
        run(
            2,
            0,
            &[0xAB],
            vec![write(1_024, &[0xAB], Err(ErrorKind::Bus)),],
        ),
        Err(EepromSlotError::Device(
            EepromError::Bus(ErrorKind::Bus)
        ))
    );
}

#[test]
fn supplied_delay_is_used_during_readiness_polling() {
    assert_eq!(
        run(
            2,
            0,
            &[0xAB],
            vec![
                write(1_024, &[0xAB], Ok(())),
                poll(1_024, Err(ADDRESS_NACK)),
                Step::Delay(1_000_000),
                poll(1_024, Ok(())),
            ],
        ),
        Ok(())
    );
}

#[test]
fn write_timeout_is_preserved() {
    let mut steps = vec![write(1_024, &[0xAB], Ok(()))];

    for _ in 0..10 {
        steps.push(poll(1_024, Err(ADDRESS_NACK)));
        steps.push(Step::Delay(1_000_000));
    }

    steps.push(poll(1_024, Err(ADDRESS_NACK)));

    assert_eq!(
        run(2, 0, &[0xAB], steps),
        Err(EepromSlotError::Device(
            EepromError::WriteTimeout
        ))
    );
}

#[test]
fn readiness_bus_error_is_preserved_without_retry() {
    assert_eq!(
        run(
            2,
            0,
            &[0xAB],
            vec![
                write(1_024, &[0xAB], Ok(())),
                poll(1_024, Err(ErrorKind::ArbitrationLoss)),
            ],
        ),
        Err(EepromSlotError::Device(
            EepromError::Bus(ErrorKind::ArbitrationLoss)
        ))
    );
}
