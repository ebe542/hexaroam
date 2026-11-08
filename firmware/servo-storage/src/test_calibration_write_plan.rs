use hexaroam_servo_control::{Angle, Calibration};

use super::{
    CalibrationWritePlan, CalibrationWritePlanError, plan_calibration_write,
};
use crate::{JournalRecord, LocatedRecord};

fn calibration(
    lower_angle: i32,
    lower_pulse: u32,
    upper_angle: i32,
    upper_pulse: u32,
) -> Calibration {
    Calibration::new(
        Angle::from_centidegrees(lower_angle),
        lower_pulse,
        Angle::from_centidegrees(upper_angle),
        upper_pulse,
    )
    .unwrap()
}

fn values() -> [Calibration; 2] {
    [
        calibration(-4_500, 1_000, 4_500, 2_000),
        calibration(-4_500, 1_200, 4_500, 2_200),
    ]
}

fn latest(
    slot_index: usize,
    sequence: u64,
    calibrations: [Option<Calibration>; 2],
) -> LocatedRecord<2> {
    LocatedRecord {
        slot_index,
        record: JournalRecord {
            sequence,
            calibrations,
        },
    }
}

fn expected(slot_index: usize, sequence: u64) -> Option<CalibrationWritePlan> {
    Some(CalibrationWritePlan {
        slot_index,
        sequence,
    })
}

#[test]
fn first_record_uses_slot_zero_and_sequence_one() {
    assert_eq!(
        plan_calibration_write(64, None, &values()),
        Ok(expected(0, 1))
    );
}

#[test]
fn changed_data_uses_next_slot_and_sequence() {
    let previous = latest(7, 25, [Some(values()[0]); 2]);

    assert_eq!(
        plan_calibration_write(64, Some(&previous), &values()),
        Ok(expected(8, 26))
    );
}

#[test]
fn last_slot_wraps_to_slot_zero() {
    let previous = latest(63, 25, [Some(values()[0]); 2]);

    assert_eq!(
        plan_calibration_write(64, Some(&previous), &values()),
        Ok(expected(0, 26))
    );
}

#[test]
fn two_slot_storage_alternates_slots() {
    for (current, target) in [(0, 1), (1, 0)] {
        let previous = latest(current, 10, [None; 2]);

        assert_eq!(
            plan_calibration_write(2, Some(&previous), &values()),
            Ok(expected(target, 11))
        );
    }
}

#[test]
fn unchanged_complete_data_requires_no_write() {
    let previous = latest(17, 30, values().map(Some));

    assert_eq!(
        plan_calibration_write(64, Some(&previous), &values()),
        Ok(None)
    );
}

#[test]
fn changes_to_either_angle_require_a_write() {
    for changed in [
        calibration(-4_000, 1_000, 4_500, 2_000),
        calibration(-4_500, 1_000, 4_000, 2_000),
    ] {
        let previous = latest(4, 20, values().map(Some));
        let mut next = values();
        next[0] = changed;

        assert_eq!(
            plan_calibration_write(64, Some(&previous), &next),
            Ok(expected(5, 21))
        );
    }
}

#[test]
fn changes_to_either_pulse_width_require_a_write() {
    for changed in [
        calibration(-4_500, 1_100, 4_500, 2_000),
        calibration(-4_500, 1_000, 4_500, 2_100),
    ] {
        let previous = latest(4, 20, values().map(Some));
        let mut next = values();
        next[1] = changed;

        assert_eq!(
            plan_calibration_write(64, Some(&previous), &next),
            Ok(expected(5, 21))
        );
    }
}

#[test]
fn incomplete_latest_record_can_be_replaced() {
    let previous = latest(8, 30, [Some(values()[0]), None]);

    assert_eq!(
        plan_calibration_write(64, Some(&previous), &values()),
        Ok(expected(9, 31))
    );
}

#[test]
fn empty_latest_record_can_be_replaced() {
    let previous = latest(8, 30, [None; 2]);

    assert_eq!(
        plan_calibration_write(64, Some(&previous), &values()),
        Ok(expected(9, 31))
    );
}

#[test]
fn sequence_overflow_is_rejected_for_changed_data() {
    let previous = latest(8, u64::MAX, [None; 2]);

    assert_eq!(
        plan_calibration_write(64, Some(&previous), &values()),
        Err(CalibrationWritePlanError::SequenceOverflow)
    );
}

#[test]
fn unchanged_data_at_maximum_sequence_still_requires_no_write() {
    let previous = latest(8, u64::MAX, values().map(Some));

    assert_eq!(
        plan_calibration_write(64, Some(&previous), &values()),
        Ok(None)
    );
}

#[test]
fn fewer_than_two_slots_are_rejected_even_for_unchanged_data() {
    let previous = latest(0, 10, values().map(Some));

    for slot_count in [0, 1] {
        assert_eq!(
            plan_calibration_write(slot_count, None, &values()),
            Err(CalibrationWritePlanError::InvalidSlotCount)
        );
        assert_eq!(
            plan_calibration_write(slot_count, Some(&previous), &values(),),
            Err(CalibrationWritePlanError::InvalidSlotCount)
        );
    }
}

#[test]
fn zero_servos_are_rejected() {
    assert_eq!(
        plan_calibration_write::<0>(64, None, &[]),
        Err(CalibrationWritePlanError::InvalidServoCount)
    );
}

#[test]
fn invalid_existing_slot_is_rejected_even_for_unchanged_data() {
    for index in [64, usize::MAX] {
        let previous = latest(index, 10, values().map(Some));

        assert_eq!(
            plan_calibration_write(64, Some(&previous), &values()),
            Err(CalibrationWritePlanError::UnknownSlot(
                index
            ))
        );
    }
}

#[test]
fn every_valid_position_selects_a_different_slot() {
    for current in 0..64 {
        let previous = latest(current, 10, [None; 2]);
        let target = if current == 63 { 0 } else { current + 1 };

        let plan = plan_calibration_write(64, Some(&previous), &values())
            .unwrap()
            .unwrap();

        assert_eq!(plan.slot_index, target);
        assert_ne!(plan.slot_index, current);
        assert_eq!(plan.sequence, 11);
    }
}
