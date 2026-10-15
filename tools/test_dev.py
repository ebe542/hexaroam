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
                side_effect=[dev.CommandResult(1, ""), dev.CommandResult(0, "")],
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
                side_effect=[dev.CommandResult(0, ""), dev.CommandResult(0, "")],
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
                side_effect=[dev.CommandResult(0, ""), dev.CommandResult(1, "")],
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




class CommandExecutionTests(unittest.TestCase):
    def execute(self, text, return_code):
        with (
            patch.object(dev.subprocess, "Popen") as popen,
            redirect_stdout(io.StringIO()) as terminal,
        ):
            process = popen.return_value.__enter__.return_value
            process.stdout = io.StringIO(text)
            process.wait.return_value = return_code
            result = dev.run_command(["cargo", "test"], Path("."))
        return result, terminal.getvalue(), popen

    def test_output_is_captured_and_displayed(self):
        result, terminal, popen = self.execute("first\nsecond\n", 0)
        self.assertEqual(result, dev.CommandResult(0, "first\nsecond\n"))
        self.assertIn("first\nsecond\n", terminal)
        options = popen.call_args.kwargs
        self.assertEqual(options["stdout"], dev.subprocess.PIPE)
        self.assertEqual(options["stderr"], dev.subprocess.STDOUT)
        self.assertTrue(options["text"])
        self.assertFalse(options["shell"])
        self.assertEqual(options["env"]["PYTHONIOENCODING"], "utf-8")
        self.assertEqual(options["env"]["CARGO_TERM_COLOR"], "never")

    def test_output_is_displayed_before_process_completion(self):
        with (
            patch.object(dev.subprocess, "Popen") as popen,
            redirect_stdout(io.StringIO()) as terminal,
        ):
            process = popen.return_value.__enter__.return_value
            process.stdout = io.StringIO("live output\n")

            def finish():
                self.assertIn("live output\n", terminal.getvalue())
                return 0

            process.wait.side_effect = finish
            dev.run_command(["cargo", "test"], Path("."))

    def test_process_failure_preserves_code_and_output(self):
        result, _, _ = self.execute("build failed\n", 7)
        self.assertEqual(result, dev.CommandResult(7, "build failed\n"))

    def test_empty_output_is_preserved(self):
        result, _, _ = self.execute("", 0)
        self.assertEqual(result, dev.CommandResult(0, ""))

    def test_missing_executable_returns_failure(self):
        with (
            patch.object(dev.subprocess, "Popen", side_effect=FileNotFoundError),
            redirect_stdout(io.StringIO()) as terminal,
        ):
            result = dev.run_command(["missing-command"], Path("."))
        self.assertNotEqual(result.return_code, 0)
        self.assertIn("not found", result.output)
        self.assertIn("missing-command", terminal.getvalue())

    def test_os_error_returns_failure(self):
        with (
            patch.object(
                dev.subprocess, "Popen", side_effect=PermissionError("denied")
            ),
            redirect_stdout(io.StringIO()),
        ):
            result = dev.run_command(["cargo"], Path("."))
        self.assertNotEqual(result.return_code, 0)
        self.assertIn("denied", result.output)


class TestTotalTests(unittest.TestCase):
    @staticmethod
    def rust_output(passed, failed=0, skipped=0, filtered=0):
        status = "FAILED" if failed else "ok"
        return (
            f"test result: {status}. {passed} passed; {failed} failed; "
            f"{skipped} ignored; 0 measured; {filtered} filtered out; "
            "finished in 0.00s\n"
        )

    def summarize(self, steps, results):
        with (
            patch.object(dev, "TASKS", {"test": steps}),
            patch.object(dev, "run_command", side_effect=results) as command,
            redirect_stdout(io.StringIO()) as output,
        ):
            return_code = dev.run_task("test")
        self.assertEqual(command.call_count, len(steps))
        return return_code, output.getvalue()

    def test_python_and_rust_counts_are_added_without_counting_build(self):
        code, output = self.summarize(
            [
                (Path("tools"), ["python", "-m", "unittest"]),
                (Path("signal"), ["cargo", "test", "--lib"]),
                (Path("control"), ["cargo", "test", "--lib"]),
                (Path("pico"), ["cargo", "build"]),
            ],
            [
                dev.CommandResult(0, "Ran 3 tests in 0.01s\n\nOK\n"),
                dev.CommandResult(0, self.rust_output(6)),
                dev.CommandResult(0, self.rust_output(21)),
                dev.CommandResult(0, self.rust_output(999)),
            ],
        )
        self.assertEqual(code, 0)
        self.assertIn("Total: 30 passed, 0 failed", output)
        self.assertIn("0 of 4 checks failed", output)

    def test_failed_and_skipped_counts_are_preserved(self):
        code, output = self.summarize(
            [(Path("signal"), ["cargo", "test"])],
            [dev.CommandResult(1, self.rust_output(4, 1, 2, 3))],
        )
        self.assertNotEqual(code, 0)
        self.assertIn(
            "Total: 4 passed, 1 failed, 2 skipped, 3 filtered out", output
        )

    def test_missing_summary_produces_only_a_known_subtotal(self):
        code, output = self.summarize(
            [
                (Path("signal"), ["cargo", "test"]),
                (Path("control"), ["cargo", "test"]),
            ],
            [
                dev.CommandResult(1, "compilation failed\n"),
                dev.CommandResult(0, self.rust_output(21)),
            ],
        )
        self.assertNotEqual(code, 0)
        self.assertIn("Test count: unknown", output)
        self.assertIn("Known subtotal: 21 passed", output)
        self.assertIn("Incomplete test totals: 1 of 2", output)
        self.assertNotIn("\nTotal:", output)

    def test_unrecognized_successful_output_stays_unknown(self):
        code, output = self.summarize(
            [(Path("control"), ["cargo", "test"])],
            [dev.CommandResult(0, "unrecognized format\n")],
        )
        self.assertEqual(code, 0)
        self.assertIn("Test count: unknown", output)
        self.assertIn("Incomplete test totals", output)

    def test_exit_code_is_authoritative_even_with_success_summary(self):
        code, output = self.summarize(
            [(Path("control"), ["cargo", "test"])],
            [dev.CommandResult(7, self.rust_output(21))],
        )
        self.assertNotEqual(code, 0)
        self.assertIn("[FAIL] control", output)
        self.assertIn("Overall: FAILED", output)

    def test_zero_tests_is_not_unknown(self):
        code, output = self.summarize(
            [(Path("control"), ["cargo", "test"])],
            [dev.CommandResult(0, self.rust_output(0))],
        )
        self.assertEqual(code, 0)
        self.assertIn("Total: 0 passed", output)
        self.assertNotIn("unknown", output)

    def test_no_test_commands_have_no_test_total(self):
        code, output = self.summarize(
            [(Path("pico"), ["cargo", "build"])],
            [dev.CommandResult(0, "")],
        )
        self.assertEqual(code, 0)
        self.assertNotIn("\nTotal:", output)
        self.assertNotIn("Known subtotal", output)

    def test_python_discovery_includes_parser_tests(self):
        pattern = dev.RUNNER_TEST_COMMAND[
            dev.RUNNER_TEST_COMMAND.index("-p") + 1
        ]
        self.assertEqual(pattern, "test_*.py")



    def test_colored_rust_summary_is_counted(self):
        text = self.rust_output(6).replace(
            "test result: ok.", "\x1b[32mtest result: ok.\x1b[0m"
        )
        code, output = self.summarize(
            [(Path("signal"), ["cargo", "test"])],
            [dev.CommandResult(0, text)],
        )
        self.assertEqual(code, 0)
        self.assertIn("Total: 6 passed", output)

    def test_colored_python_summary_is_counted(self):
        text = "Ran 3 tests in 0.01s\n\n\x1b[32mOK\x1b[0m\n"
        code, output = self.summarize(
            [(Path("tools"), ["python", "-m", "unittest"])],
            [dev.CommandResult(0, text)],
        )
        self.assertEqual(code, 0)
        self.assertIn("Total: 3 passed", output)


class ColorOutputTests(unittest.TestCase):
    def execute(self, environment):
        with (
            patch.dict(dev.os.environ, environment, clear=True),
            patch.object(dev.sys, "stdout") as terminal,
            patch.object(dev.subprocess, "Popen") as popen,
        ):
            terminal.isatty.return_value = True
            process = popen.return_value.__enter__.return_value
            process.stdout = io.StringIO("\x1b[32mok\x1b[0m\n")
            process.wait.return_value = 0
            result = dev.run_command(["cargo", "test", "--lib"], Path("."))
        return result, popen, terminal

    def test_terminal_preserves_colors_and_enables_test_colors(self):
        result, popen, terminal = self.execute({})
        self.assertEqual(result.output, "\x1b[32mok\x1b[0m\n")
        self.assertEqual(popen.call_args.kwargs["env"]["CARGO_TERM_COLOR"], "always")
        self.assertEqual(popen.call_args.kwargs["env"]["PYTHON_COLORS"], "1")
        self.assertEqual(
            popen.call_args.args[0],
            ["cargo", "test", "--lib", "--", "--color", "always"],
        )
        written = "".join(
            call.args[0] for call in terminal.write.call_args_list
        )
        self.assertIn("\x1b[32mok\x1b[0m\n", written)

    def test_no_color_opt_out_is_respected(self):
        _, popen, _ = self.execute({"NO_COLOR": "1", "FORCE_COLOR": "1"})
        environment = popen.call_args.kwargs["env"]
        self.assertEqual(environment["CARGO_TERM_COLOR"], "never")
        self.assertEqual(environment["PYTHON_COLORS"], "0")
        self.assertNotIn("FORCE_COLOR", environment)
        self.assertEqual(popen.call_args.args[0], ["cargo", "test", "--lib"])


    def test_summary_status_uses_green_and_red(self):
        with (
            patch.dict(dev.os.environ, {}, clear=True),
            patch.object(dev.sys, "stdout") as terminal,
        ):
            terminal.isatty.return_value = True
            self.assertEqual(
                dev.color_status("PASS", True), "\x1b[32mPASS\x1b[0m"
            )
            self.assertEqual(
                dev.color_status("FAIL", False), "\x1b[31mFAIL\x1b[0m"
            )

    def test_summary_respects_no_color(self):
        with (
            patch.dict(dev.os.environ, {"NO_COLOR": "1"}, clear=True),
            patch.object(dev.sys, "stdout") as terminal,
        ):
            terminal.isatty.return_value = True
            self.assertEqual(dev.color_status("PASS", True), "PASS")

    def test_redirected_summary_is_plain(self):
        with (
            patch.dict(dev.os.environ, {}, clear=True),
            redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(dev.color_status("FAIL", False), "FAIL")


    def test_summary_colors_only_status_words(self):
        captured = []

        def record(*values, **kwargs):
            captured.append(" ".join(str(value) for value in values))

        with (
            patch.dict(dev.os.environ, {}, clear=True),
            patch.object(dev.sys, "stdout") as terminal,
            patch.object(dev, "TASKS", {
                "test": [(Path("signal"), ["cargo", "test"])]
            }),
            patch.object(
                dev, "run_command",
                return_value=dev.CommandResult(1, "compilation failed"),
            ),
            patch("builtins.print", side_effect=record),
        ):
            terminal.isatty.return_value = True
            self.assertNotEqual(dev.run_task("test"), 0)

        output = "\n".join(captured)
        self.assertIn("[\x1b[31mFAIL\x1b[0m] signal: cargo test", output)
        self.assertIn("Overall: \x1b[31mFAILED\x1b[0m -", output)
        self.assertNotIn("\x1b[31m[FAIL]", output)

if __name__ == "__main__":
    unittest.main()
