"""Console value-conversion tests; no UART or hardware is used."""

import unittest
from typing import Any, cast

from hexaroam_tools.calibration_cli import format_angle, parse_angle


class AngleConversionTests(unittest.TestCase):
    def test_integer_degrees_are_converted(self):
        for text, expected in [
            ("0", 0),
            ("45", 4500),
            ("-45", -4500),
            ("+45", 4500),
        ]:
            with self.subTest(text=text):
                self.assertEqual(parse_angle(text), expected)

    def test_decimal_point_and_comma_are_supported(self):
        for text, expected in [
            ("-45.25", -4525),
            ("-45,25", -4525),
            ("12.3", 1230),
            ("12,3", 1230),
            ("0.01", 1),
            ("-0,01", -1),
            ("+0.00", 0),
        ]:
            with self.subTest(text=text):
                self.assertEqual(parse_angle(text), expected)

    def test_surrounding_whitespace_is_ignored(self):
        self.assertEqual(parse_angle("  -12,50 \t"), -1250)

    def test_i32_boundaries_are_supported(self):
        self.assertEqual(parse_angle("-21474836.48"), -(2**31))
        self.assertEqual(parse_angle("21474836.47"), 2**31 - 1)

    def test_out_of_range_angles_are_rejected(self):
        for text in ("21474836.48", "-21474836.49", "999999999"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse_angle(text)

    def test_extra_decimal_places_are_rejected_without_rounding(self):
        for text in ("1.001", "-0,001", "45.000"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse_angle(text)

    def test_invalid_input_is_rejected(self):
        for text in (
            "",
            " ",
            "+",
            "-",
            ".",
            ",",
            "1.",
            ".5",
            "1,2.3",
            "1 2",
            "nan",
            "inf",
            "1e2",
        ):
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse_angle(text)

    def test_angles_are_formatted_with_two_decimal_places(self):
        for value, expected in [
            (0, "0.00"),
            (1, "0.01"),
            (-1, "-0.01"),
            (4500, "45.00"),
            (-4525, "-45.25"),
            (2**31 - 1, "21474836.47"),
            (-(2**31), "-21474836.48"),
        ]:
            with self.subTest(value=value):
                self.assertEqual(format_angle(value), expected)

    def test_invalid_format_values_are_rejected(self):
        for value in (
            True,
            False,
            1.5,
            "4500",
            None,
            -(2**31) - 1,
            2**31,
        ):
            with self.subTest(value=value), self.assertRaises(ValueError):
                format_angle(cast(Any, value))

    def test_format_and_parse_round_trip(self):
        for value in (-(2**31), -4501, -1, 0, 1, 4501, 2**31 - 1):
            with self.subTest(value=value):
                self.assertEqual(parse_angle(format_angle(value)), value)
