"""Calibration-document tests; no serial ports or hardware are used."""

import json
import tempfile
import unittest
from pathlib import Path
from typing import Any, cast

from hexaroam_tools.calibration_data import (
    CalibrationData,
    ServoCalibration,
    from_json,
    load,
    save,
    to_json,
)


def servo(**changes) -> ServoCalibration:
    values = {
        "servo_id": 0,
        "lower_angle_centidegrees": -4500,
        "lower_pulse_us": 1000,
        "upper_angle_centidegrees": 4500,
        "upper_pulse_us": 2000,
    }
    values.update(changes)
    return ServoCalibration(**values)


def document() -> dict:
    return {
        "schema_version": 1,
        "servo_count": 1,
        "servos": [
            {
                "servo_id": 0,
                "lower_angle_centidegrees": -4500,
                "lower_pulse_us": 1000,
                "upper_angle_centidegrees": 4500,
                "upper_pulse_us": 2000,
            }
        ],
    }


class ServoCalibrationTests(unittest.TestCase):
    def test_increasing_pulses_are_accepted(self):
        calibration = servo()

        self.assertEqual(calibration.servo_id, 0)
        self.assertEqual(calibration.lower_angle_centidegrees, -4500)
        self.assertEqual(calibration.upper_angle_centidegrees, 4500)
        self.assertEqual(calibration.lower_pulse_us, 1000)
        self.assertEqual(calibration.upper_pulse_us, 2000)

    def test_decreasing_pulses_are_accepted(self):
        calibration = servo(lower_pulse_us=2000, upper_pulse_us=1000)

        self.assertEqual(calibration.lower_pulse_us, 2000)
        self.assertEqual(calibration.upper_pulse_us, 1000)

    def test_equal_or_reversed_angles_are_rejected(self):
        for lower, upper in [(0, 0), (4500, -4500)]:
            with self.subTest(lower=lower, upper=upper), self.assertRaises(ValueError):
                servo(
                    lower_angle_centidegrees=lower,
                    upper_angle_centidegrees=upper,
                )

    def test_zero_and_equal_pulses_are_rejected(self):
        for lower, upper in [(0, 2000), (1000, 0), (1500, 1500)]:
            with self.subTest(lower=lower, upper=upper), self.assertRaises(ValueError):
                servo(lower_pulse_us=lower, upper_pulse_us=upper)

    def test_fields_require_integers_and_reject_booleans(self):
        fields = (
            "servo_id",
            "lower_angle_centidegrees",
            "lower_pulse_us",
            "upper_angle_centidegrees",
            "upper_pulse_us",
        )
        for field in fields:
            for value in (True, False, 1.5, "1000", None):
                with (
                    self.subTest(field=field, value=value),
                    self.assertRaises(ValueError),
                ):
                    servo(**{field: value})

    def test_fields_respect_binary_integer_ranges(self):
        invalid = [
            ("servo_id", -1),
            ("servo_id", 2**32),
            ("lower_angle_centidegrees", -(2**31) - 1),
            ("lower_angle_centidegrees", 2**31),
            ("upper_angle_centidegrees", -(2**31) - 1),
            ("upper_angle_centidegrees", 2**31),
            ("lower_pulse_us", -1),
            ("lower_pulse_us", 2**32),
            ("upper_pulse_us", -1),
            ("upper_pulse_us", 2**32),
        ]
        for field, value in invalid:
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                servo(**{field: value})


class CalibrationDataTests(unittest.TestCase):
    def test_sparse_draft_is_accepted(self):
        data = CalibrationData(3, (servo(servo_id=2),))

        self.assertEqual(data.servo_count, 3)
        self.assertEqual(data.servos[0].servo_id, 2)

    def test_empty_draft_is_accepted(self):
        self.assertEqual(CalibrationData(18, ()).servos, ())

    def test_duplicate_servo_ids_are_rejected(self):
        with self.assertRaises(ValueError):
            CalibrationData(2, (servo(), servo()))

    def test_ids_outside_configured_count_are_rejected(self):
        with self.assertRaises(ValueError):
            CalibrationData(1, (servo(servo_id=1),))

    def test_invalid_servo_counts_are_rejected(self):
        for count in (0, -1, 2**32, True, False, 1.5, "18", None):
            with self.subTest(count=count), self.assertRaises(ValueError):
                CalibrationData(cast(Any, count), ())

    def test_entries_must_be_an_immutable_tuple_of_calibrations(self):
        for entries in ([servo()], (None,), (document()["servos"][0],)):
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                CalibrationData(1, cast(Any, entries))

    def test_completeness_requires_every_configured_servo(self):
        for entries in ((), (servo(servo_id=0),), (servo(servo_id=1),)):
            with self.subTest(entries=entries):
                data = CalibrationData(2, entries)
                with self.assertRaises(ValueError):
                    data.require_complete()

        # Input order does not change completeness.
        complete = CalibrationData(
            2,
            (servo(servo_id=1), servo(servo_id=0)),
        )
        self.assertIsNone(complete.require_complete())


class JsonDocumentTests(unittest.TestCase):
    def test_valid_document_is_decoded(self):
        data = from_json(json.dumps(document()))

        self.assertEqual(data, CalibrationData(1, (servo(),)))

    def test_unsupported_or_noninteger_version_is_rejected(self):
        for version in (0, 2, True, 1.0, "1", None):
            with self.subTest(version=version):
                raw = document()
                raw["schema_version"] = version
                with self.assertRaises(ValueError):
                    from_json(json.dumps(raw))

    def test_missing_and_unknown_document_fields_are_rejected(self):
        for field in ("schema_version", "servo_count", "servos"):
            with self.subTest(missing=field):
                raw = document()
                del raw[field]
                with self.assertRaises(ValueError):
                    from_json(json.dumps(raw))

        raw = document()
        raw["servo_cout"] = 1
        with self.assertRaises(ValueError):
            from_json(json.dumps(raw))

    def test_missing_and_unknown_servo_fields_are_rejected(self):
        for field in document()["servos"][0]:
            with self.subTest(missing=field):
                raw = document()
                del raw["servos"][0][field]
                with self.assertRaises(ValueError):
                    from_json(json.dumps(raw))

        raw = document()
        raw["servos"][0]["pulse_us"] = 1500
        with self.assertRaises(ValueError):
            from_json(json.dumps(raw))

    def test_invalid_json_shapes_are_rejected(self):
        for text in ("{", "null", "[]", "1", '"text"'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                from_json(text)

        for entries in (None, {}, "servos", [None], [7]):
            with self.subTest(entries=entries):
                raw = document()
                raw["servos"] = entries
                with self.assertRaises(ValueError):
                    from_json(json.dumps(raw))

    def test_invalid_calibration_values_are_rejected_during_loading(self):
        invalid = [
            ("servo_id", True),
            ("lower_pulse_us", 0),
            ("lower_pulse_us", "1000"),
            ("upper_angle_centidegrees", -4500),
        ]
        for field, value in invalid:
            with self.subTest(field=field, value=value):
                raw = document()
                raw["servos"][0][field] = value
                with self.assertRaises(ValueError):
                    from_json(json.dumps(raw))

    def test_encoding_contains_version_and_explicit_units(self):
        text = to_json(CalibrationData(1, (servo(),)))

        self.assertEqual(json.loads(text), document())
        self.assertIn("\n", text)
        self.assertTrue(text.endswith("\n"))

    def test_sparse_document_round_trip_preserves_data(self):
        original = CalibrationData(
            3,
            (
                servo(servo_id=2, lower_pulse_us=2000, upper_pulse_us=1000),
                servo(servo_id=0),
            ),
        )

        self.assertEqual(from_json(to_json(original)), original)

    def test_file_round_trip_creates_parent_directory(self):
        original = CalibrationData(1, (servo(),))

        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "calibration" / "servos.json"

            save(path, original)

            self.assertTrue(path.is_file())
            self.assertEqual(load(path), original)
            self.assertEqual(
                json.loads(path.read_text(encoding="utf-8")),
                document(),
            )

    def test_missing_file_reports_file_not_found(self):
        with (
            tempfile.TemporaryDirectory() as temporary,
            self.assertRaises(FileNotFoundError),
        ):
            load(Path(temporary) / "missing.json")

    def test_invalid_file_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "invalid.json"
            path.write_text('{"schema_version": 1}', encoding="utf-8")

            with self.assertRaises(ValueError):
                load(path)
