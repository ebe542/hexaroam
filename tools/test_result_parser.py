"""Tests for parsing Rust and Python test summaries."""

import unittest

from result_parser import (
    TestCounts,
    parse_python_summary,
    parse_rust_summary,
)


class RustSummaryTests(unittest.TestCase):
    def test_successful_run(self):
        output = (
            "running 6 tests\n"
            "test tests::example ... ok\n"
            "test result: ok. 6 passed; 0 failed; 0 ignored; "
            "0 measured; 0 filtered out; finished in 0.00s\n"
        )

        self.assertEqual(
            parse_rust_summary(output),
            TestCounts(passed=6, failed=0, skipped=0),
        )

    def test_failed_run(self):
        output = (
            "test result: FAILED. 5 passed; 1 failed; 0 ignored; "
            "0 measured; 0 filtered out; finished in 0.01s\n"
        )

        self.assertEqual(
            parse_rust_summary(output),
            TestCounts(passed=5, failed=1, skipped=0),
        )

    def test_ignored_and_filtered_tests_are_separate(self):
        output = (
            "test result: ok. 3 passed; 0 failed; 2 ignored; "
            "0 measured; 4 filtered out; finished in 0.00s\n"
        )

        self.assertEqual(
            parse_rust_summary(output),
            TestCounts(
                passed=3,
                failed=0,
                skipped=2,
                filtered_out=4,
            ),
        )

    def test_zero_tests_is_a_known_result(self):
        output = (
            "test result: ok. 0 passed; 0 failed; 0 ignored; "
            "0 measured; 0 filtered out; finished in 0.00s\n"
        )

        self.assertEqual(
            parse_rust_summary(output),
            TestCounts(passed=0, failed=0, skipped=0),
        )

    def test_missing_summary_is_unknown(self):
        output = "error[E0599]: method not found\n"

        self.assertIsNone(parse_rust_summary(output))

    def test_multiple_summaries_are_not_silently_combined(self):
        summary = (
            "test result: ok. 3 passed; 0 failed; 0 ignored; "
            "0 measured; 0 filtered out; finished in 0.00s\n"
        )

        self.assertIsNone(parse_rust_summary(summary + summary))


class PythonSummaryTests(unittest.TestCase):
    def test_successful_run(self):
        output = "test_example ... ok\nRan 3 tests in 0.002s\n\nOK\n"

        self.assertEqual(
            parse_python_summary(output),
            TestCounts(passed=3, failed=0, skipped=0),
        )

    def test_failures_and_errors_are_counted(self):
        output = "Ran 5 tests in 0.002s\n\nFAILED (failures=1, errors=1)\n"

        self.assertEqual(
            parse_python_summary(output),
            TestCounts(passed=3, failed=2, skipped=0),
        )

    def test_skipped_tests_are_not_counted_as_passed(self):
        output = "Ran 5 tests in 0.002s\n\nOK (skipped=2)\n"

        self.assertEqual(
            parse_python_summary(output),
            TestCounts(passed=3, failed=0, skipped=2),
        )

    def test_failure_and_skip_can_occur_together(self):
        output = "Ran 4 tests in 0.002s\n\nFAILED (failures=1, skipped=1)\n"

        self.assertEqual(
            parse_python_summary(output),
            TestCounts(passed=2, failed=1, skipped=1),
        )

    def test_single_test_summary(self):
        output = "Ran 1 test in 0.001s\n\nOK\n"

        self.assertEqual(
            parse_python_summary(output),
            TestCounts(passed=1, failed=0, skipped=0),
        )

    def test_zero_tests_is_a_known_result(self):
        output = "Ran 0 tests in 0.000s\n\nOK\n"

        self.assertEqual(
            parse_python_summary(output),
            TestCounts(passed=0, failed=0, skipped=0),
        )

    def test_missing_final_status_is_unknown(self):
        output = "Ran 3 tests in 0.002s\n"

        self.assertIsNone(parse_python_summary(output))

    def test_multiple_runs_are_not_silently_combined(self):
        output = "Ran 3 tests in 0.002s\n\nOK\n"

        self.assertIsNone(parse_python_summary(output + output))

    def test_inconsistent_counts_are_unknown(self):
        output = "Ran 2 tests in 0.002s\n\nFAILED (failures=3)\n"

        self.assertIsNone(parse_python_summary(output))


if __name__ == "__main__":
    unittest.main()
