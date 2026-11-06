use super::{ResponseError, encode_ack, encode_nack, encode_status};
use crate::{
    ByteTransport,
    cobs_frame::{CobsFrameReceiver, MAX_WIRE_SIZE},
    cobs_transmitter::CobsTransmitter,
    command::{Command, NackCode},
};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};

#[test]
fn status_response_uses_little_endian() {
    let mut output = [0xA5; 8];

    assert_eq!(
        encode_status(0x1234_5678, &mut output),
        Ok(6)
    );
    assert_eq!(
        &output[..6],
        &[0x80, 0x01, 0x78, 0x56, 0x34, 0x12]
    );
    assert_eq!(&output[6..], &[0xA5; 2]);
}

#[test]
fn status_bits_are_transmitted_without_interpretation() {
    for status in [0, 1, 0x107, u32::MAX] {
        let mut output = [0; 6];
        assert_eq!(
            encode_status(status, &mut output),
            Ok(6)
        );
        assert_eq!(&output[2..], &status.to_le_bytes());
    }
}

#[test]
fn enable_ack_contains_only_response_and_command() {
    let mut output = [0xA5; 4];

    assert_eq!(
        encode_ack(Command::OutputEnable, &mut output),
        Ok(2)
    );
    assert_eq!(output, [0x80, 0x02, 0xA5, 0xA5]);
}

#[test]
fn targets_ack_contains_only_response_and_command() {
    let mut output = [0xA5; 4];

    assert_eq!(
        encode_ack(Command::SetTargets, &mut output),
        Ok(2)
    );
    assert_eq!(output, [0x80, 0x03, 0xA5, 0xA5]);
}

#[test]
fn generic_ack_rejects_status_command_without_modifying_output() {
    let mut output = [0xA5; 6];

    assert_eq!(
        encode_ack(Command::GetStatus, &mut output),
        Err(ResponseError::MissingStatus),
    );
    assert_eq!(output, [0xA5; 6]);

    assert_eq!(
        encode_ack(Command::GetStatus, &mut []),
        Err(ResponseError::MissingStatus),
    );
}

#[test]
fn nack_preserves_unknown_original_command() {
    let mut output = [0xA5; 5];

    assert_eq!(
        encode_nack(
            0xFE,
            NackCode::UnknownCommand,
            &mut output
        ),
        Ok(3),
    );
    assert_eq!(output, [0x81, 0xFE, 0x01, 0xA5, 0xA5]);
}

#[test]
fn nack_codes_have_the_agreed_wire_values() {
    for (code, expected) in [
        (NackCode::UnknownCommand, 1),
        (NackCode::InvalidLength, 2),
        (NackCode::InvalidValue, 3),
        (NackCode::InvalidState, 4),
        (NackCode::Busy, 5),
        (NackCode::OutputError, 6),
    ] {
        let mut output = [0; 3];

        assert_eq!(
            encode_nack(0x03, code, &mut output),
            Ok(3)
        );
        assert_eq!(output, [0x81, 0x03, expected]);
    }
}

#[test]
fn short_status_buffer_is_preserved() {
    for length in 0..6 {
        let mut output = [0xA5; 6];

        assert_eq!(
            encode_status(1, &mut output[..length]),
            Err(ResponseError::OutputTooSmall),
        );
        assert_eq!(output, [0xA5; 6]);
    }
}

#[test]
fn short_ack_buffer_is_preserved() {
    for length in 0..2 {
        let mut output = [0xA5; 2];

        assert_eq!(
            encode_ack(
                Command::OutputEnable,
                &mut output[..length]
            ),
            Err(ResponseError::OutputTooSmall),
        );
        assert_eq!(output, [0xA5; 2]);
    }
}

#[test]
fn short_nack_buffer_is_preserved() {
    for length in 0..3 {
        let mut output = [0xA5; 3];

        assert_eq!(
            encode_nack(
                0x03,
                NackCode::InvalidState,
                &mut output[..length],
            ),
            Err(ResponseError::OutputTooSmall),
        );
        assert_eq!(output, [0xA5; 3]);
    }
}

#[test]
fn exact_capacity_is_sufficient_for_each_response() {
    assert_eq!(encode_status(1, &mut [0; 6]), Ok(6));
    assert_eq!(
        encode_ack(Command::OutputEnable, &mut [0; 2]),
        Ok(2),
    );
    assert_eq!(
        encode_nack(0x03, NackCode::Busy, &mut [0; 3]),
        Ok(3),
    );
}

#[test]
fn status_response_round_trips_through_transmitter_and_receiver() {
    struct Transport {
        bytes: [u8; MAX_WIRE_SIZE],
        length: usize,
    }

    impl ByteTransport for Transport {
        type Error = core::convert::Infallible;

        fn try_read(&mut self) -> Result<Option<u8>, Self::Error> {
            panic!("Transmitter must not receive")
        }

        fn try_write(&mut self, byte: u8) -> Result<bool, Self::Error> {
            self.bytes[self.length] = byte;
            self.length += 1;
            Ok(true)
        }
    }

    let mut content = [0; 6];
    let length = encode_status(0x107, &mut content).unwrap();
    let mut checksum = |bytes: &[u8]| SoftwareCrc32.checksum(bytes);

    let mut sender = CobsTransmitter::new();
    sender
        .submit(&content[..length], &mut checksum)
        .unwrap();

    let mut transport = Transport {
        bytes: [0; MAX_WIRE_SIZE],
        length: 0,
    };

    sender
        .service(&mut transport, MAX_WIRE_SIZE)
        .unwrap();
    assert!(!sender.is_pending());

    let mut receiver = CobsFrameReceiver::new();
    for &byte in &transport.bytes[..transport.length - 1] {
        assert_eq!(
            receiver.push(byte, &mut checksum),
            Ok(None)
        );
    }

    assert_eq!(
        receiver.push(0, &mut checksum),
        Ok(Some(
            &[0x80, 0x01, 0x07, 0x01, 0, 0][..]
        )),
    );
}
