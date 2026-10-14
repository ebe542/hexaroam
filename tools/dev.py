"""Run HexaRoam development tasks."""

import argparse
import subprocess
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
SIGNAL_DIR = PROJECT_ROOT / "firmware" / "servo-signal"
PICO_DIR = PROJECT_ROOT / "firmware" / "pico"

TASKS = {
    "test": [
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
    ],
    "fmt": [
        (SIGNAL_DIR, ["cargo", "fmt"]),
        (PICO_DIR, ["cargo", "fmt"]),
    ],
    "check": [
        (SIGNAL_DIR, ["cargo", "fmt", "--check"]),
        (PICO_DIR, ["cargo", "fmt", "--check"]),
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
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

    for directory, arguments in TASKS[args.task]:
        return_code = run_command(arguments, directory)

        if return_code != 0:
            return return_code

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
