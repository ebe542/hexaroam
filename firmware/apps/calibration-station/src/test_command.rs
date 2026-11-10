use super::{Command, Request, decode};
use hexaroam_servo_protocol::command::{CommandError, NackCode};

fn rejected(command: u8, code: NackCode) -> CommandError {
    CommandError::Rejected { command, code }
}

#[test]
fn command_bytes_are_stable() {
    assert_eq!(Command::GetStatus as u8, 0x01);
    assert_eq!(Command::OutputEnable as u8, 0x02);
    assert_eq!(Command::SetPulse as u8, 0x03);
}

#[test]
fn empty_content_is_rejected() {
    assert_eq!(
        decode(&[]),
        Err(CommandError::EmptyInput)
    );
}

#[test]
fn status_request_is_decoded() {
    assert_eq!(
        decode(&[0x01]),
        Ok(Some(Request::GetStatus))
    );
}

#[test]
fn status_parameters_are_rejected() {
    assert_eq!(
        decode(&[0x01, 0]),
        Err(rejected(0x01, NackCode::InvalidLength)),
    );
}

#[test]
fn enable_and_disable_are_decoded() {
    assert_eq!(
        decode(&[0x02, 0]),
        Ok(Some(Request::OutputEnable(false))),
    );
    assert_eq!(
        decode(&[0x02, 1]),
        Ok(Some(Request::OutputEnable(true))),
    );
}

#[test]
fn enable_requires_exactly_one_parameter() {
    for content in [&[0x02][..], &[0x02, 1, 0][..]] {
        assert_eq!(
            decode(content),
            Err(rejected(0x02, NackCode::InvalidLength)),
        );
    }
}

#[test]
fn enable_rejects_values_other_than_zero_or_one() {
    for value in 2..=u8::MAX {
        assert_eq!(
            decode(&[0x02, value]),
            Err(rejected(0x02, NackCode::InvalidValue)),
        );
    }
}

#[test]
fn pulse_is_decoded_as_little_endian() {
    assert_eq!(
        decode(&[0x03, 0xDC, 0x05, 0, 0]),
        Ok(Some(Request::SetPulse(1_500))),
    );
    assert_eq!(
        decode(&[0x03, 0x78, 0x56, 0x34, 0x12]),
        Ok(Some(Request::SetPulse(0x1234_5678))),
    );
}

#[test]
fn pulse_limits_are_not_checked_by_decoder() {
    for value in [0_u32, 999, 1_000, 2_000, 2_001, u32::MAX] {
        let mut content = [0x03, 0, 0, 0, 0];
        content[1..].copy_from_slice(&value.to_le_bytes());

        assert_eq!(
            decode(&content),
            Ok(Some(Request::SetPulse(value)))
        );
    }
}

#[test]
fn pulse_requires_exactly_four_parameter_bytes() {
    let content = [0x03, 0, 0, 0, 0, 0];

    for length in [1, 2, 3, 4, 6] {
        assert_eq!(
            decode(&content[..length]),
            Err(rejected(0x03, NackCode::InvalidLength)),
        );
    }
}

#[test]
fn unknown_command_is_rejected() {
    assert_eq!(
        decode(&[0x7F]),
        Err(rejected(0x7F, NackCode::UnknownCommand)),
    );
}

#[test]
fn response_codes_are_ignored() {
    for content in [
        &[0x80][..],
        &[0x81][..],
        &[0x80, 0x03][..],
        &[0x81, 0x03, 0x02][..],
    ] {
        assert_eq!(decode(content), Ok(None));
    }
}
