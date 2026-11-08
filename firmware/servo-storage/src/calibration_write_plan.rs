//! Plans one calibration write without accessing storage.

use hexaroam_servo_control::Calibration;

use crate::LocatedRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibrationWritePlan {
    pub slot_index: usize,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationWritePlanError {
    InvalidSlotCount,
    InvalidServoCount,
    UnknownSlot(usize),
    SequenceOverflow,
}

/// Plans the first write attempt for a complete dataset.
///
/// Requires at least two slots and one servo.
/// An existing record must have a valid slot index.
///
/// Identical complete data returns None without incrementing
/// the sequence. Missing entries count as changed data.
///
/// Without an existing record, select slot zero and sequence one.
/// Otherwise select the next slot in the ring and increment
/// the sequence without wrapping.
///
/// This function does not encode data or check slot capacity.
/// Encoding must succeed before storage is modified.
pub fn plan_calibration_write<const N: usize>(
    slot_count: usize,
    latest: Option<&LocatedRecord<N>>,
    calibrations: &[Calibration; N],
) -> Result<Option<CalibrationWritePlan>, CalibrationWritePlanError> {
    if slot_count < 2 {
        return Err(CalibrationWritePlanError::InvalidSlotCount);
    }

    if N == 0 {
        return Err(CalibrationWritePlanError::InvalidServoCount);
    }

    let Some(latest) = latest else {
        return Ok(Some(CalibrationWritePlan {
            slot_index: 0,
            sequence: 1,
        }));
    };

    if latest.slot_index >= slot_count {
        return Err(CalibrationWritePlanError::UnknownSlot(
            latest.slot_index,
        ));
    }

    let unchanged = latest
        .record
        .calibrations
        .iter()
        .zip(calibrations.iter())
        .all(|(previous, current)| previous.as_ref() == Some(current));

    if unchanged {
        return Ok(None);
    }

    let sequence = latest
        .record
        .sequence
        .checked_add(1)
        .ok_or(CalibrationWritePlanError::SequenceOverflow)?;

    let slot_index = if latest.slot_index == slot_count - 1 {
        0
    } else {
        latest.slot_index + 1
    };

    Ok(Some(CalibrationWritePlan {
        slot_index,
        sequence,
    }))
}

#[cfg(test)]
#[path = "test_calibration_write_plan.rs"]
mod tests;
