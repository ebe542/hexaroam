"""Tests for the development task runner."""

import io
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import dev


class TaskSummaryTests(unittest.TestCase):
    def test_failure_is_preserved_when_last_step_passes(self):
        tasks = {
            "test": [
                (
                    Path("firmware/servo-signal"),
                    ["cargo", "test", "--lib", "--locked"],
                ),
                (
                    Path("firmware/servo-control"),
                    ["cargo", "test", "--lib", "--locked"],
                ),
            ],
        }
        output = io.StringIO()

        with (
            patch.object(dev, "TASKS", tasks),
            patch.object(
                dev,
                "run_command",
                side_effect=[1, 0],
            ) as run_command,
            redirect_stdout(output),
        ):
            return_code = dev.run_task("test")

        self.assertEqual(run_command.call_count, 2)
        self.assertNotEqual(return_code, 0)

        summary = output.getvalue()
        self.assertIn("[FAIL] servo-signal", summary)
        self.assertIn("[PASS] servo-control", summary)
        self.assertIn("Overall: FAILED", summary)

    def test_all_steps_passing_returns_success(self):
        tasks = {
            "test": [
                (Path("firmware/servo-signal"), ["cargo", "test"]),
                (Path("firmware/servo-control"), ["cargo", "test"]),
            ],
        }
        output = io.StringIO()

        with (
            patch.object(dev, "TASKS", tasks),
            patch.object(
                dev,
                "run_command",
                side_effect=[0, 0],
            ) as run_command,
            redirect_stdout(output),
        ):
            return_code = dev.run_task("test")

        self.assertEqual(run_command.call_count, 2)
        self.assertEqual(return_code, 0)

        summary = output.getvalue()
        self.assertIn("[PASS] servo-signal", summary)
        self.assertIn("[PASS] servo-control", summary)
        self.assertIn("Overall: PASSED", summary)
        self.assertIn("0 of 2 checks failed", summary)

    def test_last_step_failure_returns_failure(self):
        tasks = {
            "test": [
                (Path("firmware/servo-signal"), ["cargo", "test"]),
                (Path("firmware/servo-control"), ["cargo", "test"]),
            ],
        }
        output = io.StringIO()

        with (
            patch.object(dev, "TASKS", tasks),
            patch.object(
                dev,
                "run_command",
                side_effect=[0, 1],
            ) as run_command,
            redirect_stdout(output),
        ):
            return_code = dev.run_task("test")

        self.assertEqual(run_command.call_count, 2)
        self.assertNotEqual(return_code, 0)

        summary = output.getvalue()
        self.assertIn("[PASS] servo-signal", summary)
        self.assertIn("[FAIL] servo-control", summary)
        self.assertIn("Overall: FAILED", summary)
        self.assertIn("1 of 2 checks failed", summary)


if __name__ == "__main__":
    unittest.main()
