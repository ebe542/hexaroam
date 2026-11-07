use super::{CAPACITY, Eeprom, EepromError};
use embedded_hal::i2c::{Error, ErrorKind, ErrorType, I2c, Operation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BusError;

impl Error for BusError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

struct TestBus {
    address: u8,
    offset: u16,
    length: usize,
    calls: usize,
    fail: bool,
}

impl TestBus {
    fn new(address: u8, offset: u16, length: usize) -> Self {
        Self {
            address,
            offset,
            length,
            calls: 0,
            fail: false,
        }
    }
}

impl ErrorType for TestBus {
    type Error = BusError;
}

impl I2c for TestBus {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        self.calls += 1;
        assert_eq!(address, self.address);

        let [Operation::Write(pointer), Operation::Read(output)] = operations
        else {
            panic!("Expected one address-write/read transaction");
        };

        assert_eq!(*pointer, &self.offset.to_be_bytes());
        assert_eq!(output.len(), self.length);

        if self.fail {
            return Err(BusError);
        }

        for (index, byte) in output.iter_mut().enumerate() {
            *byte = index as u8;
        }

        Ok(())
    }
}

#[test]
fn capacity_is_32_kib() {
    assert_eq!(CAPACITY, 32_768);
}

#[test]
fn all_supported_device_addresses_are_accepted_without_bus_access() {
    for address in 0x50..=0x57 {
        let mut bus = TestBus::new(address, 0, 0);

        {
            let _driver = Eeprom::new(&mut bus, address).unwrap();
        }

        assert_eq!(bus.calls, 0);
    }
}

#[test]
fn unsupported_device_addresses_are_rejected_without_bus_access() {
    for address in [0x00, 0x4F, 0x58, 0xA0, 0xFF] {
        let mut bus = TestBus::new(address, 0, 0);
        let result = Eeprom::new(&mut bus, address);

        assert!(matches!(
            result,
            Err(EepromError::InvalidDeviceAddress)
        ));
        assert_eq!(bus.calls, 0);
    }
}

#[test]
fn reads_first_byte() {
    let mut bus = TestBus::new(0x50, 0, 1);
    let mut output = [0xAA];

    Eeprom::new(&mut bus, 0x50)
        .unwrap()
        .read(0, &mut output)
        .unwrap();

    assert_eq!(output, [0]);
    assert_eq!(bus.calls, 1);
}

#[test]
fn sends_memory_address_high_byte_first() {
    let mut bus = TestBus::new(0x53, 0x1234, 3);
    let mut output = [0xAA; 3];

    Eeprom::new(&mut bus, 0x53)
        .unwrap()
        .read(0x1234, &mut output)
        .unwrap();

    assert_eq!(output, [0, 1, 2]);
    assert_eq!(bus.calls, 1);
}

#[test]
fn sequential_read_can_cross_a_write_page_boundary() {
    let mut bus = TestBus::new(0x50, 63, 3);
    let mut output = [0xAA; 3];

    Eeprom::new(&mut bus, 0x50)
        .unwrap()
        .read(63, &mut output)
        .unwrap();

    assert_eq!(output, [0, 1, 2]);
    assert_eq!(bus.calls, 1);
}

#[test]
fn reads_last_byte() {
    let mut bus = TestBus::new(0x57, 0x7FFF, 1);
    let mut output = [0xAA];

    Eeprom::new(&mut bus, 0x57)
        .unwrap()
        .read(0x7FFF, &mut output)
        .unwrap();

    assert_eq!(output, [0]);
    assert_eq!(bus.calls, 1);
}

#[test]
fn read_can_end_exactly_at_capacity() {
    let mut bus = TestBus::new(0x50, 0x7FFE, 2);
    let mut output = [0xAA; 2];

    Eeprom::new(&mut bus, 0x50)
        .unwrap()
        .read(0x7FFE, &mut output)
        .unwrap();

    assert_eq!(output, [0, 1]);
    assert_eq!(bus.calls, 1);
}

#[test]
fn read_beyond_end_is_rejected_without_modifying_output() {
    let mut bus = TestBus::new(0x50, 0, 0);
    let mut output = [0xAA; 2];

    let result = Eeprom::new(&mut bus, 0x50)
        .unwrap()
        .read(0x7FFF, &mut output);

    assert_eq!(result, Err(EepromError::OutOfBounds));
    assert_eq!(output, [0xAA; 2]);
    assert_eq!(bus.calls, 0);
}

#[test]
fn nonempty_read_at_or_above_capacity_is_rejected() {
    for offset in [0x8000, 0xFFFF] {
        let mut bus = TestBus::new(0x50, 0, 0);
        let mut output = [0xAA];

        let result = Eeprom::new(&mut bus, 0x50)
            .unwrap()
            .read(offset, &mut output);

        assert_eq!(result, Err(EepromError::OutOfBounds));
        assert_eq!(output, [0xAA]);
        assert_eq!(bus.calls, 0);
    }
}

#[test]
fn empty_reads_within_bounds_do_not_access_bus() {
    for offset in [0, 0x1234, 0x7FFF, 0x8000] {
        let mut bus = TestBus::new(0x50, 0, 0);

        Eeprom::new(&mut bus, 0x50)
            .unwrap()
            .read(offset, &mut [])
            .unwrap();

        assert_eq!(bus.calls, 0);
    }
}

#[test]
fn empty_read_beyond_capacity_is_rejected() {
    let mut bus = TestBus::new(0x50, 0, 0);

    let result = Eeprom::new(&mut bus, 0x50)
        .unwrap()
        .read(0x8001, &mut []);

    assert_eq!(result, Err(EepromError::OutOfBounds));
    assert_eq!(bus.calls, 0);
}

#[test]
fn bus_error_is_forwarded_without_retry() {
    let mut bus = TestBus::new(0x50, 0x1234, 2);
    bus.fail = true;
    let mut output = [0xAA; 2];

    let result = Eeprom::new(&mut bus, 0x50)
        .unwrap()
        .read(0x1234, &mut output);

    assert_eq!(result, Err(EepromError::Bus(BusError)));
    assert_eq!(bus.calls, 1);
}
