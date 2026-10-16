"""Run HexaRoam development tasks with live output and test totals."""

import argparse
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

from result_parser import TestCounts, parse_python_summary, parse_rust_summary

PROJECT_ROOT = Path(__file__).resolve().parent.parent
SIGNAL_DIR = PROJECT_ROOT / "firmware" / "servo-signal"
PICO_DIR = PROJECT_ROOT / "firmware" / "pico"
CONTROL_DIR = PROJECT_ROOT / "firmware" / "servo-control"
SERVO_PIO_DIR = PROJECT_ROOT / "firmware" / "servo-pio"

RUNNER_TEST_COMMAND = [
    sys.executable,
    "-m",
    "unittest",
    "discover",
    "-s",
    "tools",
    "-p",
    "test_*.py",
    "-v",
]

TASKS = {
    "test": [
        (PROJECT_ROOT, RUNNER_TEST_COMMAND),
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (CONTROL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (SERVO_PIO_DIR, ["cargo", "test", "--lib", "--locked"]),
    ],
    "fmt": [
        (SIGNAL_DIR, ["cargo", "fmt"]),
        (CONTROL_DIR, ["cargo", "fmt"]),
        (PICO_DIR, ["cargo", "fmt"]),
        (SERVO_PIO_DIR, ["cargo", "fmt"]),
    ],
    "check": [
        (PROJECT_ROOT, RUNNER_TEST_COMMAND),
        (SIGNAL_DIR, ["cargo", "fmt", "--check"]),
        (CONTROL_DIR, ["cargo", "fmt", "--check"]),
        (PICO_DIR, ["cargo", "fmt", "--check"]),
        (SERVO_PIO_DIR, ["cargo", "fmt", "--check"]),
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (CONTROL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (PICO_DIR, ["cargo", "build", "--locked"]),
        (SERVO_PIO_DIR, ["cargo", "test", "--lib", "--locked"]),
    ],
    "uf2": [
        (PICO_DIR, ["cargo", "run", "--locked"]),
    ],
}


@dataclass(frozen=True)
class CommandResult:
    return_code: int
    output: str


def run_command(arguments: list[str], directory: Path) -> CommandResult:
    print(f"[{directory.name}] {' '.join(arguments)}", flush=True)
    environment = os.environ.copy()
    use_color = "NO_COLOR" not in environment and (
        sys.stdout.isatty() or environment.get("GITHUB_ACTIONS") == "true"
    )
    environment.update(
        {
            "CARGO_TERM_COLOR": "always" if use_color else "never",
            "PYTHON_COLORS": "1" if use_color else "0",
            "PYTHONUNBUFFERED": "1",
            "PYTHONIOENCODING": "utf-8",
        }
    )
    command = list(arguments)
    if use_color:
        environment["FORCE_COLOR"] = "1"
        if test_kind(arguments) == "rust" and "--color" not in command:
            if "--" not in command:
                command.append("--")
            command.extend(["--color", "always"])
    else:
        environment["NO_COLOR"] = "1"
        environment.pop("FORCE_COLOR", None)
    lines = []

    try:
        with subprocess.Popen(
            command,
            cwd=directory,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            env=environment,
            shell=False,
            bufsize=1,
        ) as process:
            assert process.stdout is not None
            for line in process.stdout:
                print(line, end="", flush=True)
                lines.append(line)
            return_code = process.wait()
    except FileNotFoundError:
        message = (
            f"Command or working directory not found: {arguments[0]} ({directory})"
        )
        print(message, flush=True)
        return CommandResult(1, message + "\n")
    except OSError as error:
        message = f"Could not execute {arguments[0]}: {error}"
        print(message, flush=True)
        return CommandResult(1, message + "\n")

    return CommandResult(return_code, "".join(lines))


def test_kind(arguments: list[str]) -> Optional[str]:
    if arguments[:2] == ["cargo", "test"]:
        return "rust"
    if arguments[1:3] == ["-m", "unittest"]:
        return "python"
    return None


def format_counts(counts: TestCounts) -> str:
    return (
        f"{counts.passed} passed, {counts.failed} failed, "
        f"{counts.skipped} skipped, {counts.filtered_out} filtered out"
    )


def color_status(text: str, success: bool) -> str:
    use_color = "NO_COLOR" not in os.environ and (
        sys.stdout.isatty() or os.environ.get("GITHUB_ACTIONS") == "true"
    )
    if not use_color:
        return text
    code = "32" if success else "31"
    return f"\x1b[{code}m{text}\x1b[0m"


def run_task(task: str) -> int:
    results = []
    for directory, arguments in TASKS[task]:
        result = run_command(arguments, directory)
        results.append((directory, arguments, result))

    print("\nSummary", flush=True)
    failures = 0
    test_groups = 0
    unknown_groups = 0
    passed = failed = skipped = filtered_out = 0

    for directory, arguments, result in results:
        status = "PASS" if result.return_code == 0 else "FAIL"
        print(
            f"[{color_status(status, result.return_code == 0)}] "
            f"{directory.name}: {' '.join(arguments)}",
            flush=True,
        )
        failures += result.return_code != 0
        kind = test_kind(arguments)
        if kind is None:
            continue

        test_groups += 1
        plain_output = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", result.output)
        counts = (
            parse_rust_summary(plain_output)
            if kind == "rust"
            else parse_python_summary(plain_output)
        )
        if counts is None:
            unknown_groups += 1
            print("       Test count: unknown", flush=True)
            continue

        print(
            f"       Tests: {format_counts(counts)}",
            flush=True,
        )
        passed += counts.passed
        failed += counts.failed
        skipped += counts.skipped
        filtered_out += counts.filtered_out

    if test_groups:
        label = "Known subtotal" if unknown_groups else "Total"
        totals = TestCounts(passed, failed, skipped, filtered_out)
        print(f"\n{label}: {format_counts(totals)}", flush=True)
        if unknown_groups:
            print(
                f"Incomplete test totals: {unknown_groups} of "
                f"{test_groups} test groups have unknown counts",
                flush=True,
            )

    overall = "FAILED" if failures else "PASSED"
    print(
        f"\nOverall: {color_status(overall, failures == 0)} - "
        f"{failures} of {len(results)} checks failed",
        flush=True,
    )
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run HexaRoam development tasks.",
    )
    parser.add_argument("task", choices=sorted(TASKS))
    args = parser.parse_args()
    return run_task(args.task)


if __name__ == "__main__":
    raise SystemExit(main())
