"""Tests for application selection; Cargo is never executed."""

import io
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch

import dev


class ApplicationSelectionTests(unittest.TestCase):
    def test_shared_checks_include_calibration_station(self):
        expected = {
            "fmt": [["cargo", "fmt"]],
            "check": [
                ["cargo", "fmt", "--check"],
                ["cargo", "check", "--locked"],
            ],
            "verify": [
                ["cargo", "fmt", "--check"],
                ["cargo", "build", "--locked"],
                ["cargo", "test", "--lib", "--locked", "--target", "host-tuple"],
            ],
        }

        for task, commands in expected.items():
            with self.subTest(task=task):
                actual = [
                    command
                    for directory, command in dev.TASKS[task]
                    if directory == dev.APP_CALIBRATION_STATION_DIR
                ]
                self.assertEqual(actual, commands)

    def test_documentation_includes_calibration_station_library(self):
        self.assertIn(
            (dev.APP_CALIBRATION_STATION_DIR, "calibration-station"),
            dev.DOC_PACKAGES,
        )

        commands = [
            command
            for directory, command in dev.TASKS["doc"]
            if directory == dev.APP_CALIBRATION_STATION_DIR
        ]

        self.assertEqual(len(commands), 1)
        self.assertEqual(
            commands[0][-1:],
            ["--lib"],
        )
        self.assertIn("--locked", commands[0])
        self.assertIn("--no-deps", commands[0])

    def test_station_library_tests_use_host_target_in_both_tasks(self):
        expected = [
            "cargo",
            "test",
            "--lib",
            "--locked",
            "--target",
            "host-tuple",
        ]
        for task in ("test", "verify"):
            with self.subTest(task=task):
                tests = [
                    command
                    for directory, command in dev.TASKS[task]
                    if directory == dev.APP_CALIBRATION_STATION_DIR
                    and command[:2] == ["cargo", "test"]
                ]
                self.assertEqual(tests, [expected])

    def test_build_defaults_to_servo_controller(self):
        self.assertEqual(
            dev.select_tasks("build", None),
            [
                (
                    dev.APP_SERVO_CONTROLLER_DIR,
                    ["cargo", "build", "--locked"],
                )
            ],
        )

    def test_build_selects_calibration_station(self):
        self.assertEqual(
            dev.select_tasks("build", "calibration-station"),
            [
                (
                    dev.APP_CALIBRATION_STATION_DIR,
                    ["cargo", "build", "--locked"],
                )
            ],
        )

    def test_uf2_selects_calibration_station(self):
        self.assertEqual(
            dev.select_tasks("uf2", "calibration-station"),
            [
                (
                    dev.APP_CALIBRATION_STATION_DIR,
                    ["cargo", "run", "--locked"],
                )
            ],
        )

    def test_hardware_build_defaults_to_both_applications(self):
        tasks = dev.select_tasks("hardware-build", None)

        self.assertEqual(
            [directory for directory, _ in tasks],
            [
                dev.APP_SERVO_CONTROLLER_DIR,
                dev.APP_CALIBRATION_STATION_DIR,
            ],
        )
        for _, command in tasks:
            self.assertEqual(
                command,
                [
                    "cargo",
                    "build",
                    "--locked",
                    "--examples",
                    "--features",
                    "hardware-tests",
                    "--message-format=json-render-diagnostics",
                ],
            )

    def test_hardware_build_selects_one_application(self):
        tasks = dev.select_tasks("hardware-build", "calibration-station")

        self.assertEqual(len(tasks), 1)
        self.assertEqual(tasks[0][0], dev.APP_CALIBRATION_STATION_DIR)
        self.assertIn("--examples", tasks[0][1])

    def test_shared_task_preserves_existing_steps(self):
        for task in ("fmt", "check", "verify", "doc", "test", "install"):
            with self.subTest(task=task):
                self.assertEqual(
                    dev.select_tasks(task, None),
                    dev.TASKS[task],
                )

    def test_shared_task_rejects_application_filter(self):
        for task in ("fmt", "check", "verify", "doc", "test", "install"):
            with self.subTest(task=task):
                with self.assertRaises(ValueError):
                    dev.select_tasks(task, "calibration-station")

    def test_unknown_application_is_rejected(self):
        with self.assertRaises(ValueError):
            dev.select_tasks("build", "unknown-app")

    def test_unknown_task_is_rejected(self):
        with self.assertRaises(ValueError):
            dev.select_tasks("unknown-task", None)


class HardwareTestDiscoveryTests(unittest.TestCase):
    def test_missing_directory_has_no_tests(self):
        with tempfile.TemporaryDirectory() as temporary:
            self.assertFalse(dev.has_hardware_tests(Path(temporary)))

    def test_empty_directory_has_no_tests(self):
        with tempfile.TemporaryDirectory() as temporary:
            application = Path(temporary)
            (application / "hardware_tests").mkdir()

            self.assertFalse(dev.has_hardware_tests(application))

    def test_readme_and_support_files_are_not_tests(self):
        with tempfile.TemporaryDirectory() as temporary:
            application = Path(temporary)
            tests = application / "hardware_tests"
            support = tests / "support"
            support.mkdir(parents=True)
            (tests / "README.md").write_text("Hardware tests", encoding="utf-8")
            (support / "mod.rs").write_text("// Support", encoding="utf-8")

            self.assertFalse(dev.has_hardware_tests(application))

    def test_direct_rust_source_is_detected(self):
        with tempfile.TemporaryDirectory() as temporary:
            application = Path(temporary)
            tests = application / "hardware_tests"
            tests.mkdir()
            (tests / "pulse_output.rs").write_text(
                "// Hardware test",
                encoding="utf-8",
            )

            self.assertTrue(dev.has_hardware_tests(application))


class HardwareBuildSkippingTests(unittest.TestCase):
    def test_application_without_tests_is_skipped(self):
        with (
            patch.object(
                dev,
                "select_tasks",
                return_value=[
                    (
                        dev.APP_CALIBRATION_STATION_DIR,
                        ["cargo", "build"],
                    )
                ],
            ),
            patch.object(dev, "has_hardware_tests", return_value=False),
            patch.object(dev, "run_command") as command,
            patch.object(dev, "hardware_reports") as reports,
            redirect_stdout(io.StringIO()) as output,
        ):
            code = dev.run_task("hardware-build")

        self.assertEqual(code, 0)
        command.assert_not_called()
        reports.assert_not_called()
        self.assertIn(
            "[SKIP] calibration-station: no hardware-test sources",
            output.getvalue(),
        )

        summary = output.getvalue().split("\nSummary\n", 1)[1]
        self.assertIn(
            "[SKIP] calibration-station: no hardware-test sources",
            summary,
        )

    def test_skipped_application_does_not_hide_other_build_failure(self):
        with (
            patch.object(
                dev,
                "select_tasks",
                return_value=[
                    (
                        dev.APP_SERVO_CONTROLLER_DIR,
                        ["cargo", "build"],
                    ),
                    (
                        dev.APP_CALIBRATION_STATION_DIR,
                        ["cargo", "build"],
                    ),
                ],
            ),
            patch.object(
                dev,
                "has_hardware_tests",
                side_effect=[True, False],
            ),
            patch.object(
                dev,
                "run_command",
                return_value=dev.CommandResult(1, "Build failed"),
            ) as command,
            redirect_stdout(io.StringIO()) as output,
        ):
            code = dev.run_task("hardware-build")

        self.assertEqual(code, 1)
        command.assert_called_once_with(
            ["cargo", "build"],
            dev.APP_SERVO_CONTROLLER_DIR,
        )
        self.assertIn(
            "[SKIP] calibration-station: no hardware-test sources",
            output.getvalue(),
        )
        self.assertIn("1 of 1 checks failed", output.getvalue())

        summary = output.getvalue().split("\nSummary\n", 1)[1]
        self.assertIn(
            "[SKIP] calibration-station: no hardware-test sources",
            summary,
        )
