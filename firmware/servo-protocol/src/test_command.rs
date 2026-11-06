use super::{Command, CommandError, NackCode, Request, TargetBatch, decode};

fn rejected(command: u8, code: NackCode) -> CommandError {
    CommandError::Rejected { command, code }
}

fn targets(content: &[u8]) -> TargetBatch<'_> {
    match decode(content).unwrap() {
        Some(Request::SetTargets(batch)) => batch,
        other => panic!("Expected target batch, received {other:?}"),
    }
}

#[test]
fn empty_input_has_no_command_for_nack() {
    assert_eq!(
        decode(&[]),
        Err(CommandError::EmptyInput)
    );
}

#[test]
fn status_request_has_no_parameters() {
    assert_eq!(
        decode(&[Command::GetStatus as u8]),
        Ok(Some(Request::GetStatus)),
    );
}

#[test]
fn status_request_rejects_parameters() {
    assert_eq!(
        decode(&[0x01, 0]),
        Err(rejected(0x01, NackCode::InvalidLength)),
    );
}

#[test]
fn enable_parameter_accepts_only_boolean_values() {
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
    for content in [&[0x02][..], &[0x02, 0, 0][..]] {
        assert_eq!(
            decode(content),
            Err(rejected(0x02, NackCode::InvalidLength)),
        );
    }
}

#[test]
fn enable_rejects_every_non_boolean_value() {
    for value in 2..=u8::MAX {
        assert_eq!(
            decode(&[0x02, value]),
            Err(rejected(0x02, NackCode::InvalidValue)),
        );
    }
}

#[test]
fn unknown_commands_are_rejected() {
    for command in 0..=u8::MAX {
        if matches!(command, 0x01..=0x03 | 0x80 | 0x81) {
            continue;
        }

        assert_eq!(
            decode(&[command]),
            Err(rejected(
                command,
                NackCode::UnknownCommand
            )),
        );
    }
}

#[test]
fn incoming_ack_and_nack_are_ignored_without_reply() {
    for content in [
        &[0x80][..],
        &[0x80, 0x01, 0, 0, 0, 0][..],
        &[0x81][..],
        &[0x81, 0x03, 0x05][..],
    ] {
        assert_eq!(decode(content), Ok(None));
    }
}

#[test]
fn single_target_uses_little_endian_signed_angle() {
    // -1000 centidegrees = -10 degrees.
    let batch = targets(&[0x03, 4, 0x18, 0xFC, 0xFF, 0xFF]);

    assert_eq!(batch.first_servo, 4);
    assert_eq!(batch.angle_count(), 1);
    assert_eq!(batch.angle(0), Some(-1_000));
}

#[test]
fn consecutive_targets_preserve_order() {
    let content = [
        0x03, 6, 0, 0, 0, 0, 0xE8, 0x03, 0, 0, 0x18, 0xFC, 0xFF, 0xFF,
    ];
    let batch = targets(&content);

    assert_eq!(batch.first_servo, 6);
    assert_eq!(batch.angle_count(), 3);
    assert_eq!(batch.angle(0), Some(0));
    assert_eq!(batch.angle(1), Some(1_000));
    assert_eq!(batch.angle(2), Some(-1_000));
}

#[test]
fn target_request_requires_start_servo() {
    assert_eq!(
        decode(&[0x03]),
        Err(rejected(0x03, NackCode::InvalidLength)),
    );
}

#[test]
fn target_request_requires_at_least_one_angle() {
    assert_eq!(
        decode(&[0x03, 0]),
        Err(rejected(0x03, NackCode::InvalidLength)),
    );
}

#[test]
fn target_request_rejects_incomplete_angle_bytes() {
    for length in [1, 2, 3, 5, 6, 7] {
        let content = [0x03, 0, 0, 0, 0, 0, 0, 0, 0];

        assert_eq!(
            decode(&content[..2 + length]),
            Err(rejected(0x03, NackCode::InvalidLength)),
        );
    }
}

#[test]
fn maximum_batch_contains_62_angles() {
    let mut content = [0; 250];
    content[0] = 0x03;

    let batch = targets(&content);

    assert_eq!(batch.angle_count(), 62);
    assert_eq!(batch.angle(61), Some(0));
    assert_eq!(batch.angle(62), None);
}

#[test]
fn batch_exceeding_frame_content_limit_is_rejected() {
    let mut content = [0; 254];
    content[0] = 0x03;

    assert_eq!(
        decode(&content),
        Err(rejected(0x03, NackCode::InvalidLength)),
    );
}

#[test]
fn decoding_does_not_apply_calibrated_angle_limits() {
    let mut content = [0; 10];
    content[0] = 0x03;
    content[2..6].copy_from_slice(&i32::MIN.to_le_bytes());
    content[6..10].copy_from_slice(&i32::MAX.to_le_bytes());

    let batch = targets(&content);

    assert_eq!(batch.angle(0), Some(i32::MIN));
    assert_eq!(batch.angle(1), Some(i32::MAX));
}

#[test]
fn decoding_does_not_apply_physical_servo_count() {
    let batch = targets(&[0x03, 255, 0, 0, 0, 0]);

    assert_eq!(batch.first_servo, 255);
    assert_eq!(batch.angle(0), Some(0));
}

#[test]
fn out_of_range_angle_index_returns_none_without_overflow() {
    let batch = targets(&[0x03, 0, 0, 0, 0, 0]);

    assert_eq!(batch.angle(1), None);
    assert_eq!(batch.angle(usize::MAX), None);
}
