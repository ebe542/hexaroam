use super::*;

fn angle(centidegrees: i32) -> Angle {
    Angle::from_centidegrees(centidegrees)
}

#[test]
fn increasing_and_decreasing_pulses_are_accepted() {
    for (lower_pulse, upper_pulse) in [(1_000, 2_000), (2_000, 1_000)] {
        assert!(
            Calibration::new(
                angle(-4_500),
                lower_pulse,
                angle(4_500),
                upper_pulse,
            )
            .is_ok()
        );
    }
}

#[test]
fn equal_or_reversed_angles_are_rejected() {
    for (lower, upper) in [(0, 0), (4_500, -4_500)] {
        assert_eq!(
            Calibration::new(angle(lower), 1_000, angle(upper), 2_000,),
            Err(CalibrationError::InvalidAngleRange),
        );
    }
}

#[test]
fn zero_pulse_at_either_endpoint_is_rejected() {
    for (lower_pulse, upper_pulse) in [(0, 2_000), (1_000, 0)] {
        assert_eq!(
            Calibration::new(
                angle(-4_500),
                lower_pulse,
                angle(4_500),
                upper_pulse,
            ),
            Err(CalibrationError::ZeroPulseWidth),
        );
    }
}

#[test]
fn equal_pulse_widths_are_rejected() {
    assert_eq!(
        Calibration::new(
            angle(-4_500),
            1_500,
            angle(4_500),
            1_500,
        ),
        Err(CalibrationError::EqualPulseWidths),
    );
}

#[test]
fn lower_angle_returns_lower_pulse() {
    let calibration = Calibration::new(
        angle(-4_500),
        1_000,
        angle(4_500),
        2_000,
    )
    .expect("Valid calibration");

    assert_eq!(
        calibration.pulse_width_us(angle(-4_500)),
        Ok(1_000),
    );
}

#[test]
fn upper_angle_returns_upper_pulse() {
    let calibration = Calibration::new(
        angle(-4_500),
        1_000,
        angle(4_500),
        2_000,
    )
    .expect("Valid calibration");

    assert_eq!(
        calibration.pulse_width_us(angle(4_500)),
        Ok(2_000),
    );
}

#[test]
fn midpoint_angle_returns_midpoint_pulse() {
    let calibration = Calibration::new(
        angle(-4_500),
        1_000,
        angle(4_500),
        2_000,
    )
    .expect("Valid calibration");

    assert_eq!(
        calibration.pulse_width_us(angle(0)),
        Ok(1_500),
    );
}

#[test]
fn angle_above_upper_limit_is_rejected() {
    let calibration = Calibration::new(
        angle(-4_500),
        1_000,
        angle(4_500),
        2_000,
    )
    .expect("Valid calibration");

    assert_eq!(
        calibration.pulse_width_us(angle(4_501)),
        Err(CalibrationError::AngleOutOfRange),
    );
}

#[test]
fn angle_below_lower_limit_is_rejected() {
    let calibration = Calibration::new(
        angle(-4_500),
        1_000,
        angle(4_500),
        2_000,
    )
    .expect("Valid calibration");

    assert_eq!(
        calibration.pulse_width_us(angle(-4_501)),
        Err(CalibrationError::AngleOutOfRange),
    );
}

#[test]
fn half_microsecond_rounds_to_larger_pulse() {
    let calibration = Calibration::new(angle(0), 1_000, angle(200), 1_001)
        .expect("Valid calibration");

    assert_eq!(
        calibration.pulse_width_us(angle(100)),
        Ok(1_001),
    );
}

#[test]
fn decreasing_pulses_are_interpolated_and_rounded() {
    let calibration = Calibration::new(angle(0), 2_000, angle(400), 1_002)
        .expect("Valid calibration");

    for (target, expected) in [
        (0, 2_000),
        (100, 1_751),
        (200, 1_501),
        (300, 1_252),
        (400, 1_002),
    ] {
        assert_eq!(
            calibration.pulse_width_us(angle(target)),
            Ok(expected),
        );
    }
}

#[test]
fn full_input_ranges_are_interpolated_without_overflow() {
    let calibration = Calibration::new(
        angle(i32::MIN),
        1,
        angle(i32::MAX),
        u32::MAX,
    )
    .expect("Valid calibration");

    for (target, expected) in
        [(i32::MIN, 1), (0, 2_147_483_648), (i32::MAX, u32::MAX)]
    {
        assert_eq!(
            calibration.pulse_width_us(angle(target)),
            Ok(expected),
        );
    }
}
