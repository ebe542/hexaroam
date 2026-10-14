"""Run HexaRoam development tasks."""

import argparse
import subprocess
import sys
from pathlib import Path

RUNNER_TEST_COMMAND = [
    sys.executable,
    "-m",
    "unittest",
    "discover",
    "-s",
    "tools",
    "-p",
    "test_dev.py",
    "-v",
]
PROJECT_ROOT = Path(__file__).resolve().parent.parent
SIGNAL_DIR = PROJECT_ROOT / "firmware" / "servo-signal"
PICO_DIR = PROJECT_ROOT / "firmware" / "pico"
CONTROL_DIR = PROJECT_ROOT / "firmware" / "servo-control"

TASKS = {
    "test": [
        (PROJECT_ROOT, RUNNER_TEST_COMMAND),
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (CONTROL_DIR, ["cargo", "test", "--lib", "--locked"]),
    ],
    "fmt": [
        (SIGNAL_DIR, ["cargo", "fmt"]),
        (CONTROL_DIR, ["cargo", "fmt"]),
        (PICO_DIR, ["cargo", "fmt"]),
    ],
    "check": [
        (PROJECT_ROOT, RUNNER_TEST_COMMAND),
        (SIGNAL_DIR, ["cargo", "fmt", "--check"]),
        (CONTROL_DIR, ["cargo", "fmt", "--check"]),
        (PICO_DIR, ["cargo", "fmt", "--check"]),
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (CONTROL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (PICO_DIR, ["cargo", "build", "--locked"]),
    ],
    "uf2": [
        (PICO_DIR, ["cargo", "run", "--locked"]),
    ],
}


def run_command(arguments: list[str], directory: Path) -> int:
    print(f"[{directory.name}] {' '.join(arguments)}", flush=True)

    try:
        result = subprocess.run(
            arguments,
            cwd=directory,
            check=False,
        )
    except FileNotFoundError:
        print(f"Command not found: {arguments[0]}")
        return 1

    return result.returncode


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run HexaRoam development tasks.",
    )
    parser.add_argument("task", choices=sorted(TASKS))
    args = parser.parse_args()

    return run_task(args.task)


def run_task(task: str) -> int:
    results = []

    for directory, arguments in TASKS[task]:
        return_code = run_command(arguments, directory)
        results.append((directory, arguments, return_code))

    print("\nSummary", flush=True)

    failures = 0

    for directory, arguments, return_code in results:
        status = "PASS" if return_code == 0 else "FAIL"
        command = " ".join(arguments)

        print(
            f"[{status}] {directory.name}: {command}",
            flush=True,
        )

        if return_code != 0:
            failures += 1

    overall = "FAILED" if failures else "PASSED"

    print(
        f"\nOverall: {overall} - {failures} of {len(results)} checks failed",
        flush=True,
    )

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
