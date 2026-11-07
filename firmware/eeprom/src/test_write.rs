extern crate std;

use std::{cell::RefCell, collections::VecDeque, rc::Rc, vec, vec::Vec};

use embedded_hal::{
    delay::DelayNs,
    i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation},
};

use super::{Eeprom, EepromError, PAGE_SIZE};

const ADDRESS_NACK: ErrorKind =
    ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address);

const UNKNOWN_NACK: ErrorKind =
    ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown);

const DATA_NACK: ErrorKind =
    ErrorKind::NoAcknowledge(NoAcknowledgeSource::Data);

enum Step {
    Write(Vec<u8>, Result<(), ErrorKind>),
    Delay(u32),
}

type Script = Rc<RefCell<VecDeque<Step>>>;

struct TestBus {
    address: u8,
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
        assert_eq!(address, self.address);

        let [Operation::Write(actual)] = operations else {
            panic!("Expected exactly one write operation");
        };

        let step = self
            .script
            .borrow_mut()
            .pop_front()
            .expect("Unexpected bus access");

        match step {
            Step::Write(expected, result) => {
                assert_eq!(*actual, expected.as_slice());
                result
            }
            Step::Delay(_) => {
                panic!("Expected a delay before the next bus access");
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
            Step::Write(..) => {
                panic!("Expected a bus access before the next delay");
            }
        }
    }
}

fn transfer(offset: u16, data: &[u8], result: Result<(), ErrorKind>) -> Step {
    let mut bytes = offset.to_be_bytes().to_vec();
    bytes.extend_from_slice(data);
    Step::Write(bytes, result)
}

fn probe(offset: u16, result: Result<(), ErrorKind>) -> Step {
    transfer(offset, &[], result)
}

fn run(
    address: u8,
    offset: u16,
    data: &[u8],
    steps: Vec<Step>,
) -> Result<(), EepromError<ErrorKind>> {
    let script = Rc::new(RefCell::new(VecDeque::from(steps)));

    let bus = TestBus {
        address,
        script: Rc::clone(&script),
    };

    let mut delay = TestDelay {
        script: Rc::clone(&script),
    };

    let result = Eeprom::new(bus, address)
        .unwrap()
        .write(offset, data, &mut delay);

    assert!(
        script.borrow().is_empty(),
        "Missing expected operations"
    );
    result
}

#[test]
fn page_size_is_64_bytes() {
    assert_eq!(PAGE_SIZE, 64);
}

#[test]
fn writes_one_byte_and_checks_readiness_without_delay() {
    assert_eq!(
        run(
            0x50,
            0,
            &[0xAB],
            vec![transfer(0, &[0xAB], Ok(())), probe(0, Ok(())),],
        ),
        Ok(())
    );
}

#[test]
fn uses_configured_device_address_and_big_endian_memory_address() {
    assert_eq!(
        run(
            0x57,
            0x1234,
            &[0xAB, 0xCD],
            vec![
                transfer(0x1234, &[0xAB, 0xCD], Ok(())),
                probe(0x1234, Ok(())),
            ],
        ),
        Ok(())
    );
}

#[test]
fn full_aligned_page_uses_one_page_write() {
    let data = [0xAA; 64];

    assert_eq!(
        run(
            0x50,
            64,
            &data,
            vec![transfer(64, &data, Ok(())), probe(64, Ok(())),],
        ),
        Ok(())
    );
}

#[test]
fn splits_at_page_boundary_and_waits_before_next_page() {
    assert_eq!(
        run(
            0x50,
            63,
            &[0x11, 0x22, 0x33],
            vec![
                transfer(63, &[0x11], Ok(())),
                probe(63, Err(ADDRESS_NACK)),
                Step::Delay(1_000_000),
                probe(63, Ok(())),
                transfer(64, &[0x22, 0x33], Ok(())),
                probe(64, Ok(())),
            ],
        ),
        Ok(())
    );
}

#[test]
fn splits_unaligned_data_across_multiple_pages() {
    let data: Vec<u8> = (0..130)
        .map(|value| value as u8)
        .collect();

    assert_eq!(
        run(
            0x50,
            62,
            &data,
            vec![
                transfer(62, &data[..2], Ok(())),
                probe(62, Ok(())),
                transfer(64, &data[2..66], Ok(())),
                probe(64, Ok(())),
                transfer(128, &data[66..], Ok(())),
                probe(128, Ok(())),
            ],
        ),
        Ok(())
    );
}

#[test]
fn writes_last_byte() {
    assert_eq!(
        run(
            0x50,
            0x7FFF,
            &[0xAB],
            vec![transfer(0x7FFF, &[0xAB], Ok(())), probe(0x7FFF, Ok(())),],
        ),
        Ok(())
    );
}

#[test]
fn write_can_end_exactly_at_capacity() {
    assert_eq!(
        run(
            0x50,
            0x7FFE,
            &[0xAB, 0xCD],
            vec![
                transfer(0x7FFE, &[0xAB, 0xCD], Ok(())),
                probe(0x7FFE, Ok(())),
            ],
        ),
        Ok(())
    );
}

#[test]
fn validates_entire_range_before_writing_any_page() {
    let data = [0xAA; 65];

    assert_eq!(
        run(0x50, 0x7FC0, &data, vec![]),
        Err(EepromError::OutOfBounds)
    );
}

#[test]
fn nonempty_writes_at_or_above_capacity_are_rejected() {
    for offset in [0x8000, 0xFFFF] {
        assert_eq!(
            run(0x50, offset, &[0xAA], vec![]),
            Err(EepromError::OutOfBounds)
        );
    }
}

#[test]
fn empty_writes_within_bounds_do_not_access_bus_or_delay() {
    for offset in [0, 0x1234, 0x7FFF, 0x8000] {
        assert_eq!(run(0x50, offset, &[], vec![]), Ok(()));
    }
}

#[test]
fn empty_write_beyond_capacity_is_rejected() {
    assert_eq!(
        run(0x50, 0x8001, &[], vec![]),
        Err(EepromError::OutOfBounds)
    );
}

#[test]
fn readiness_retries_address_and_unknown_nacks() {
    for error in [ADDRESS_NACK, UNKNOWN_NACK] {
        assert_eq!(
            run(
                0x50,
                0,
                &[0xAA],
                vec![
                    transfer(0, &[0xAA], Ok(())),
                    probe(0, Err(error)),
                    Step::Delay(1_000_000),
                    probe(0, Ok(())),
                ],
            ),
            Ok(())
        );
    }
}

#[test]
fn persistent_readiness_nack_returns_timeout() {
    let mut steps = vec![transfer(0, &[0xAA], Ok(()))];

    for _ in 0..10 {
        steps.push(probe(0, Err(ADDRESS_NACK)));
        steps.push(Step::Delay(1_000_000));
    }

    steps.push(probe(0, Err(ADDRESS_NACK)));

    assert_eq!(
        run(0x50, 0, &[0xAA], steps),
        Err(EepromError::WriteTimeout)
    );
}

#[test]
fn readiness_can_succeed_after_the_final_allowed_delay() {
    let mut steps = vec![transfer(0, &[0xAA], Ok(()))];

    for _ in 0..10 {
        steps.push(probe(0, Err(ADDRESS_NACK)));
        steps.push(Step::Delay(1_000_000));
    }

    steps.push(probe(0, Ok(())));

    assert_eq!(run(0x50, 0, &[0xAA], steps), Ok(()));
}

#[test]
fn other_readiness_errors_are_forwarded_without_retry() {
    for error in [
        DATA_NACK,
        ErrorKind::Bus,
        ErrorKind::ArbitrationLoss,
        ErrorKind::Overrun,
        ErrorKind::Other,
    ] {
        assert_eq!(
            run(
                0x50,
                0,
                &[0xAA],
                vec![transfer(0, &[0xAA], Ok(())), probe(0, Err(error)),],
            ),
            Err(EepromError::Bus(error))
        );
    }
}

#[test]
fn page_write_errors_are_forwarded_without_retry_or_polling() {
    for error in [ADDRESS_NACK, DATA_NACK, ErrorKind::Bus] {
        assert_eq!(
            run(
                0x50,
                0,
                &[0xAA],
                vec![transfer(0, &[0xAA], Err(error))],
            ),
            Err(EepromError::Bus(error))
        );
    }
}

#[test]
fn failure_on_second_page_prevents_further_writes() {
    let data = [0xAA; 129];

    assert_eq!(
        run(
            0x50,
            0,
            &data,
            vec![
                transfer(0, &data[..64], Ok(())),
                probe(0, Ok(())),
                transfer(64, &data[64..128], Err(ErrorKind::Bus)),
            ],
        ),
        Err(EepromError::Bus(ErrorKind::Bus))
    );
}

#[test]
fn readiness_timeout_prevents_writing_the_next_page() {
    let data = [0xAA; 65];
    let mut steps = vec![transfer(0, &data[..64], Ok(()))];

    for _ in 0..10 {
        steps.push(probe(0, Err(ADDRESS_NACK)));
        steps.push(Step::Delay(1_000_000));
    }

    steps.push(probe(0, Err(ADDRESS_NACK)));

    assert_eq!(
        run(0x50, 0, &data, steps),
        Err(EepromError::WriteTimeout)
    );
}
