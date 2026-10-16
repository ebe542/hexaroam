use super::*;

#[test]
fn centidegrees_preserve_the_input() {
    for value in [i32::MIN, -4_525, 0, 4_525, i32::MAX] {
        assert_eq!(
            Angle::from_centidegrees(value).as_centidegrees(),
            value,
        );
    }
}

#[test]
fn whole_degrees_are_converted_to_centidegrees() {
    for (degrees, expected) in [(-45, -4_500), (0, 0), (45, 4_500)] {
        let angle = Angle::from_degrees(degrees).expect("Valid angle");

        assert_eq!(angle.as_centidegrees(), expected);
    }
}

#[test]
fn largest_representable_whole_degree_values_are_accepted() {
    for degrees in [i32::MIN / 100, i32::MAX / 100] {
        let angle = Angle::from_degrees(degrees).expect("Valid angle");

        assert_eq!(angle.as_centidegrees(), degrees * 100);
    }
}

#[test]
fn whole_degree_overflow_is_rejected() {
    for degrees in [i32::MIN, i32::MIN / 100 - 1, i32::MAX / 100 + 1, i32::MAX]
    {
        assert_eq!(
            Angle::from_degrees(degrees),
            Err(AngleError::Overflow),
        );
    }
}
