//! Saves complete calibration with bounded slot rotation.

use hexaroam_servo_control::Calibration;

use crate::{
    CalibrationLoadError, CalibrationWritePlan, CalibrationWritePlanError,
    Checksum, SlotWrite, SlotWriteError, plan_calibration_write,
    scan_calibrations, write_calibration_slot,
};

#[derive(Debug, PartialEq, Eq)]
pub enum CalibrationSaveError<E> {
    Scan(CalibrationLoadError<E>),
    Plan(CalibrationWritePlanError),
    Write(SlotWriteError<E>),
    AttemptsExhausted {
        attempts: usize,
        last_error: SlotWriteError<E>,
    },
}

/// Saves a complete dataset with at most three distinct target slots.
///
/// Requires at least two slots and one servo.
/// Invalid configuration is rejected before storage access.
/// Unchanged data returns success without writing.
///
/// The initially newest valid slot remains protected throughout
/// this call. Failed target slots are not reused within this call.
///
/// Only invalidation/readback mismatches permit another attempt.
/// Storage and encoding errors abort immediately.
///
/// Before another attempt, rescan storage. Its sequence must exceed
/// both the newest valid sequence and the previous attempted sequence.
/// Sequence numbers never wrap.
///
/// Success requires a successful verified slot write.
pub fn save_calibrations<const N: usize, const SLOTS: usize, S: SlotWrite>(
    storage: &mut S,
    calibrations: &[Calibration; N],
    checksum: &mut impl Checksum,
) -> Result<(), CalibrationSaveError<S::Error>> {
    if SLOTS < 2 {
        return Err(CalibrationSaveError::Plan(
            CalibrationWritePlanError::InvalidSlotCount,
        ));
    }

    if N == 0 {
        return Err(CalibrationSaveError::Plan(
            CalibrationWritePlanError::InvalidServoCount,
        ));
    }

    let latest = scan_calibrations::<N, SLOTS, _>(storage, checksum)
        .map_err(CalibrationSaveError::Scan)?;

    let Some(mut plan) =
        plan_calibration_write(SLOTS, latest.as_ref(), calibrations)
            .map_err(CalibrationSaveError::Plan)?
    else {
        return Ok(());
    };

    // Preserve the original record throughout this save operation,
    // even if a failed attempt leaves another valid record behind.
    let protected_slot = latest
        .as_ref()
        .map(|record| record.slot_index);

    const MAX_ATTEMPTS: usize = 3;

    let mut attempted_slots = [usize::MAX; MAX_ATTEMPTS];
    let mut attempts = 0;

    loop {
        attempted_slots[attempts] = plan.slot_index;
        attempts += 1;

        match write_calibration_slot(
            storage,
            plan.slot_index,
            plan.sequence,
            calibrations,
            checksum,
        ) {
            Ok(()) => return Ok(()),

            Err(
                error @ (SlotWriteError::InvalidationFailed
                | SlotWriteError::VerificationFailed),
            ) => {
                if attempts == MAX_ATTEMPTS {
                    return Err(
                        CalibrationSaveError::AttemptsExhausted {
                            attempts,
                            last_error: error,
                        },
                    );
                }

                let latest =
                    scan_calibrations::<N, SLOTS, _>(storage, checksum)
                        .map_err(CalibrationSaveError::Scan)?;

                // Also avoid overwriting the newest record seen
                // during this rescan.
                let newest_slot = latest
                    .as_ref()
                    .map(|record| record.slot_index);

                let mut candidate = plan.slot_index;

                let next_slot = (0..SLOTS).find_map(|_| {
                    candidate = if candidate == SLOTS - 1 {
                        0
                    } else {
                        candidate + 1
                    };

                    if Some(candidate) == protected_slot
                        || Some(candidate) == newest_slot
                        || attempted_slots[..attempts].contains(&candidate)
                    {
                        None
                    } else {
                        Some(candidate)
                    }
                });

                let Some(slot_index) = next_slot else {
                    return Err(
                        CalibrationSaveError::AttemptsExhausted {
                            attempts,
                            last_error: error,
                        },
                    );
                };

                // A failed candidate may be invalid and therefore
                // absent from the scan. Never reuse its sequence.
                let highest_sequence =
                    latest
                        .as_ref()
                        .map_or(plan.sequence, |record| {
                            record
                                .record
                                .sequence
                                .max(plan.sequence)
                        });

                let sequence = highest_sequence.checked_add(1).ok_or(
                    CalibrationSaveError::Plan(
                        CalibrationWritePlanError::SequenceOverflow,
                    ),
                )?;

                plan = CalibrationWritePlan {
                    slot_index,
                    sequence,
                };
            }

            Err(error) => {
                return Err(CalibrationSaveError::Write(error));
            }
        }
    }
}

#[cfg(test)]
#[path = "test_calibration_save.rs"]
mod tests;
