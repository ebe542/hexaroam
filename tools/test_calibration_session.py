"""Measurement capture tests; no files, serial ports or hardware are used."""

import unittest
from typing import Any, cast

from hexaroam_tools.calibration_data import CalibrationData, ServoCalibration
from hexaroam_tools.calibration_session import CalibrationSession, MeasurementPoint


def servo(servo_id: int = 0) -> ServoCalibration:
    return ServoCalibration(
        servo_id=servo_id,
        lower_angle_centidegrees=-4500,
        lower_pulse_us=1000,
        upper_angle_centidegrees=4500,
        upper_pulse_us=2000,
    )


def session() -> CalibrationSession:
    return CalibrationSession(CalibrationData(1, ()), 0)


def capture(current: CalibrationSession) -> None:
    current.record_lower(-4500, 1000)
    current.record_upper(4500, 2000)


class MeasurementPointTests(unittest.TestCase):
    def test_valid_point_preserves_values(self):
        point = MeasurementPoint(-4500, 1000)

        self.assertEqual(point.angle_centidegrees, -4500)
        self.assertEqual(point.pulse_width_us, 1000)

    def test_binary_integer_boundaries_are_accepted(self):
        for angle, pulse in [
            (-(2**31), 1),
            (2**31 - 1, 2**32 - 1),
        ]:
            with self.subTest(angle=angle, pulse=pulse):
                point = MeasurementPoint(angle, pulse)
                self.assertEqual(point.angle_centidegrees, angle)
                self.assertEqual(point.pulse_width_us, pulse)

    def test_invalid_values_and_types_are_rejected(self):
        invalid: list[tuple[object, object]] = [
            (-(2**31) - 1, 1000),
            (2**31, 1000),
            (0, 0),
            (0, -1),
            (0, 2**32),
        ]

        for value in (True, False, 1.5, "1000", None):
            invalid.append((value, 1000))
            invalid.append((0, value))

        for angle, pulse in invalid:
            with (
                self.subTest(angle=angle, pulse=pulse),
                self.assertRaises(ValueError),
            ):
                MeasurementPoint(cast(Any, angle), cast(Any, pulse))


class CalibrationSessionTests(unittest.TestCase):
    def test_session_starts_without_points_or_document_changes(self):
        original = CalibrationData(2, (servo(1),))
        current = CalibrationSession(original, 0)

        self.assertIsNone(current.lower_point)
        self.assertIsNone(current.upper_point)
        self.assertEqual(original, CalibrationData(2, (servo(1),)))

    def test_invalid_servo_ids_are_rejected(self):
        original = CalibrationData(2, ())

        for servo_id in (-1, 2, True, False, 1.5, "0", None):
            with (
                self.subTest(servo_id=servo_id),
                self.assertRaises(ValueError),
            ):
                CalibrationSession(original, cast(Any, servo_id))

    def test_two_points_create_a_servo_entry(self):
        current = session()
        capture(current)

        self.assertEqual(current.lower_point, MeasurementPoint(-4500, 1000))
        self.assertEqual(current.upper_point, MeasurementPoint(4500, 2000))
        self.assertEqual(current.finish(), CalibrationData(1, (servo(),)))

    def test_upper_point_can_be_recorded_first(self):
        current = session()
        current.record_upper(4500, 2000)
        current.record_lower(-4500, 1000)

        self.assertEqual(current.finish(), CalibrationData(1, (servo(),)))

    def test_incomplete_sessions_cannot_finish(self):
        for recorded in ("none", "lower", "upper"):
            with self.subTest(recorded=recorded):
                current = session()

                if recorded == "lower":
                    current.record_lower(-4500, 1000)
                elif recorded == "upper":
                    current.record_upper(4500, 2000)

                with self.assertRaises(ValueError):
                    current.finish()

    def test_existing_servo_points_are_not_reused(self):
        original = CalibrationData(1, (servo(),))
        current = CalibrationSession(original, 0)

        self.assertIsNone(current.lower_point)
        self.assertIsNone(current.upper_point)

        current.record_lower(-3000, 1100)

        with self.assertRaises(ValueError):
            current.finish()

        self.assertEqual(original.servos, (servo(),))

    def test_replacement_preserves_other_servos_and_original_document(self):
        original = CalibrationData(2, (servo(1), servo(0)))
        current = CalibrationSession(original, 1)

        current.record_lower(-3000, 1100)
        current.record_upper(3000, 1900)
        updated = current.finish()

        expected = ServoCalibration(1, -3000, 1100, 3000, 1900)
        entries = {entry.servo_id: entry for entry in updated.servos}

        self.assertEqual(updated.servo_count, 2)
        self.assertEqual(len(updated.servos), 2)
        self.assertEqual(entries[0], servo(0))
        self.assertEqual(entries[1], expected)
        self.assertEqual(original.servos, (servo(1), servo(0)))
        self.assertIsNot(updated, original)

    def test_decreasing_pulses_are_supported(self):
        current = session()
        current.record_lower(-4500, 2000)
        current.record_upper(4500, 1000)

        self.assertEqual(
            current.finish(),
            CalibrationData(
                1,
                (ServoCalibration(0, -4500, 2000, 4500, 1000),),
            ),
        )

    def test_invalid_pair_preserves_both_previous_points(self):
        current = session()
        capture(current)

        invalid = [
            ("lower", 4500, 1000),
            ("lower", 5000, 1000),
            ("lower", -4500, 2000),
            ("upper", -4500, 2000),
            ("upper", -5000, 2000),
            ("upper", 4500, 1000),
        ]

        for which, angle, pulse in invalid:
            with self.subTest(which=which, angle=angle, pulse=pulse):
                record = (
                    current.record_lower if which == "lower" else current.record_upper
                )

                with self.assertRaises(ValueError):
                    record(angle, pulse)

                self.assertEqual(current.lower_point, MeasurementPoint(-4500, 1000))
                self.assertEqual(current.upper_point, MeasurementPoint(4500, 2000))
                self.assertEqual(current.finish(), CalibrationData(1, (servo(),)))

    def test_invalid_individual_point_preserves_previous_capture(self):
        current = session()
        current.record_lower(-4500, 1000)

        with self.assertRaises(ValueError):
            current.record_lower(-3000, 0)

        self.assertEqual(current.lower_point, MeasurementPoint(-4500, 1000))
        self.assertIsNone(current.upper_point)

    def test_latest_valid_measurement_replaces_previous_point(self):
        current = session()
        capture(current)

        current.record_lower(-3000, 1100)
        current.record_upper(3000, 1900)

        self.assertEqual(
            current.finish(),
            CalibrationData(
                1,
                (ServoCalibration(0, -3000, 1100, 3000, 1900),),
            ),
        )

    def test_finishing_one_servo_can_leave_document_incomplete(self):
        original = CalibrationData(18, ())
        current = CalibrationSession(original, 5)
        capture(current)

        updated = current.finish()

        self.assertEqual(updated.servo_count, 18)
        self.assertEqual(updated.servos, (servo(5),))
        self.assertEqual(original.servos, ())

        with self.assertRaises(ValueError):
            updated.require_complete()

    def test_repeated_finish_returns_equivalent_data(self):
        current = session()
        capture(current)

        first = current.finish()
        second = current.finish()

        self.assertEqual(first, second)
        self.assertEqual(first, CalibrationData(1, (servo(),)))


if __name__ == "__main__":
    unittest.main()
