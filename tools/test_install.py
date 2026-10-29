"""Installation task tests; no tools are installed by these tests."""

import io
import unittest
from contextlib import redirect_stdout
from unittest.mock import patch

import dev


class InstallTests(unittest.TestCase):
    def test_installs_required_components_target_and_pinned_host_converter(self):
        self.assertEqual(dev.TASKS["install"], [
            (dev.PICO_DIR, ["rustup", "component", "add", "rustfmt", "clippy", "llvm-tools-preview"]),
            (dev.PICO_DIR, ["rustup", "target", "add", "thumbv6m-none-eabi"]),
            (dev.PICO_DIR, ["cargo", "install", "elf2uf2-rs", "--version", "2.2.0",
                            "--locked", "--target", "host-tuple"]),
        ])

    def run_install(self, results):
        with (
            patch.object(dev, "run_command", side_effect=results) as command,
            # These tests check task status, not terminal/CI color policy.
            # Color behavior has its own tests in test_dev.py.
            patch.object(dev, "color_status", side_effect=lambda text, success: text),
            redirect_stdout(io.StringIO()) as output,
        ):
            code = dev.run_task("install")
        self.assertEqual(command.call_count, 3)
        for actual, (directory, arguments) in zip(
            command.call_args_list, dev.TASKS["install"],
        ):
            self.assertEqual(actual.args, (arguments, directory))
        return code, output.getvalue()

    def test_success_has_no_test_totals(self):
        code, output = self.run_install([dev.CommandResult(0, "") for _ in range(3)])
        self.assertEqual(code, 0)
        self.assertIn("0 of 3 checks failed", output)
        self.assertNotIn("\nTotal:", output)

    def test_component_failure_is_preserved_after_successful_later_steps(self):
        code, output = self.run_install([
            dev.CommandResult(1, "component unavailable"),
            dev.CommandResult(0, "installed"),
            dev.CommandResult(0, "already installed"),
        ])
        self.assertNotEqual(code, 0)
        self.assertIn("1 of 3 checks failed", output)

    def test_converter_failure_is_reported(self):
        code, output = self.run_install([
            dev.CommandResult(0, ""), dev.CommandResult(0, ""),
            dev.CommandResult(1, "installation failed"),
        ])
        self.assertNotEqual(code, 0)
        self.assertIn("[FAIL] pico: cargo install", output)


if __name__ == "__main__":
    unittest.main()
