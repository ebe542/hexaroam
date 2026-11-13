"""Run HexaRoam development tasks with live output and test totals."""

import argparse
import html
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

from result_parser import TestCounts, parse_python_summary, parse_rust_summary

PROJECT_ROOT = Path(__file__).resolve().parent.parent

APP_SERVO_CONTROLLER_DIR = PROJECT_ROOT / "firmware" / "apps" / "servo-controller"
APP_CALIBRATION_STATION_DIR = PROJECT_ROOT / "firmware" / "apps" / "calibration-station"

SIGNAL_DIR = PROJECT_ROOT / "firmware" / "servo-signal"
CONTROL_DIR = PROJECT_ROOT / "firmware" / "servo-control"
SERVO_PIO_DIR = PROJECT_ROOT / "firmware" / "servo-pio"
LOGGING_DIR = PROJECT_ROOT / "firmware" / "logging"
STORAGE_DIR = PROJECT_ROOT / "firmware" / "servo-storage"
PROTOCOL_DIR = PROJECT_ROOT / "firmware" / "servo-protocol"
REGISTERS_DIR = PROJECT_ROOT / "firmware" / "servo-registers"
EEPROM_DIR = PROJECT_ROOT / "firmware" / "eeprom"
SERVO_EEPROM_DIR = PROJECT_ROOT / "firmware" / "servo-eeprom"

DOC_TARGET = "thumbv6m-none-eabi"
DOC_TARGET_DIR = PROJECT_ROOT / "target" / "rustdoc"
DOC_OUTPUT_DIR = DOC_TARGET_DIR / DOC_TARGET / "doc"
DOC_PACKAGES = (
    (APP_SERVO_CONTROLLER_DIR, "servo-controller"),
    (APP_CALIBRATION_STATION_DIR, "calibration-station"),
    (SIGNAL_DIR, "hexaroam-servo-signal"),
    (CONTROL_DIR, "hexaroam-servo-control"),
    (SERVO_PIO_DIR, "hexaroam-servo-pio"),
    (LOGGING_DIR, "hexaroam-logging"),
    (STORAGE_DIR, "hexaroam-servo-storage"),
    (PROTOCOL_DIR, "hexaroam-servo-protocol"),
    (REGISTERS_DIR, "hexaroam-servo-registers"),
    (EEPROM_DIR, "hexaroam-eeprom"),
    (SERVO_EEPROM_DIR, "hexaroam-servo-eeprom"),
)

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
    "doc": [
        (
            directory,
            [
                "cargo",
                "doc",
                "--locked",
                "--no-deps",
                "--document-private-items",
                "--target",
                DOC_TARGET,
                "--target-dir",
                str(DOC_TARGET_DIR),
                *(
                    ["--bin", name]
                    if directory == APP_SERVO_CONTROLLER_DIR
                    else ["--lib"]
                ),
            ],
        )
        for directory, name in DOC_PACKAGES
    ],
    "install": [
        (
            APP_SERVO_CONTROLLER_DIR,
            ["rustup", "component", "add", "rustfmt", "clippy", "llvm-tools-preview"],
        ),
        (APP_SERVO_CONTROLLER_DIR, ["rustup", "target", "add", "thumbv6m-none-eabi"]),
        (
            APP_SERVO_CONTROLLER_DIR,
            [
                "cargo",
                "install",
                "elf2uf2-rs",
                "--version",
                "2.2.0",
                "--locked",
                "--target",
                "host-tuple",
            ],
        ),
    ],
    "test": [
        (PROJECT_ROOT, RUNNER_TEST_COMMAND),
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (CONTROL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (SERVO_PIO_DIR, ["cargo", "test", "--lib", "--locked"]),
        (LOGGING_DIR, ["cargo", "test", "--lib", "--locked"]),
        (STORAGE_DIR, ["cargo", "test", "--lib", "--locked"]),
        (PROTOCOL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (REGISTERS_DIR, ["cargo", "test", "--lib", "--locked"]),
        (EEPROM_DIR, ["cargo", "test", "--lib", "--locked"]),
        (SERVO_EEPROM_DIR, ["cargo", "test", "--lib", "--locked"]),
        (
            APP_CALIBRATION_STATION_DIR,
            ["cargo", "test", "--lib", "--locked", "--target", "host-tuple"],
        ),
    ],
    "fmt": [
        (APP_SERVO_CONTROLLER_DIR, ["cargo", "fmt"]),
        (APP_CALIBRATION_STATION_DIR, ["cargo", "fmt"]),
        (SIGNAL_DIR, ["cargo", "fmt"]),
        (CONTROL_DIR, ["cargo", "fmt"]),
        (SERVO_PIO_DIR, ["cargo", "fmt"]),
        (LOGGING_DIR, ["cargo", "fmt"]),
        (STORAGE_DIR, ["cargo", "fmt"]),
        (PROTOCOL_DIR, ["cargo", "fmt"]),
        (REGISTERS_DIR, ["cargo", "fmt"]),
        (EEPROM_DIR, ["cargo", "fmt"]),
        (SERVO_EEPROM_DIR, ["cargo", "fmt"]),
    ],
    "verify": [
        (PROJECT_ROOT, RUNNER_TEST_COMMAND),
        (APP_SERVO_CONTROLLER_DIR, ["cargo", "fmt", "--check"]),
        (APP_SERVO_CONTROLLER_DIR, ["cargo", "build", "--locked"]),
        (APP_CALIBRATION_STATION_DIR, ["cargo", "fmt", "--check"]),
        (APP_CALIBRATION_STATION_DIR, ["cargo", "build", "--locked"]),
        (SIGNAL_DIR, ["cargo", "fmt", "--check"]),
        (CONTROL_DIR, ["cargo", "fmt", "--check"]),
        (SERVO_PIO_DIR, ["cargo", "fmt", "--check"]),
        (SIGNAL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (CONTROL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (SERVO_PIO_DIR, ["cargo", "test", "--lib", "--locked"]),
        (LOGGING_DIR, ["cargo", "fmt", "--check"]),
        (LOGGING_DIR, ["cargo", "test", "--lib", "--locked"]),
        (STORAGE_DIR, ["cargo", "fmt", "--check"]),
        (STORAGE_DIR, ["cargo", "test", "--lib", "--locked"]),
        (PROTOCOL_DIR, ["cargo", "fmt", "--check"]),
        (PROTOCOL_DIR, ["cargo", "test", "--lib", "--locked"]),
        (REGISTERS_DIR, ["cargo", "fmt", "--check"]),
        (REGISTERS_DIR, ["cargo", "test", "--lib", "--locked"]),
        (EEPROM_DIR, ["cargo", "fmt", "--check"]),
        (EEPROM_DIR, ["cargo", "test", "--lib", "--locked"]),
        (SERVO_EEPROM_DIR, ["cargo", "fmt", "--check"]),
        (SERVO_EEPROM_DIR, ["cargo", "test", "--lib", "--locked"]),
        (
            APP_CALIBRATION_STATION_DIR,
            ["cargo", "test", "--lib", "--locked", "--target", "host-tuple"],
        ),
    ],
    "check": [
        (APP_SERVO_CONTROLLER_DIR, ["cargo", "fmt", "--check"]),
        (APP_SERVO_CONTROLLER_DIR, ["cargo", "check", "--locked"]),
        (APP_CALIBRATION_STATION_DIR, ["cargo", "fmt", "--check"]),
        (APP_CALIBRATION_STATION_DIR, ["cargo", "check", "--locked"]),
        (SIGNAL_DIR, ["cargo", "fmt", "--check"]),
        (CONTROL_DIR, ["cargo", "fmt", "--check"]),
        (SERVO_PIO_DIR, ["cargo", "fmt", "--check"]),
        (SIGNAL_DIR, ["cargo", "check", "--locked"]),
        (CONTROL_DIR, ["cargo", "check", "--locked"]),
        (SERVO_PIO_DIR, ["cargo", "check", "--locked"]),
        (LOGGING_DIR, ["cargo", "fmt", "--check"]),
        (LOGGING_DIR, ["cargo", "check", "--locked"]),
        (STORAGE_DIR, ["cargo", "fmt", "--check"]),
        (STORAGE_DIR, ["cargo", "check", "--locked"]),
        (PROTOCOL_DIR, ["cargo", "fmt", "--check"]),
        (PROTOCOL_DIR, ["cargo", "check", "--locked"]),
        (REGISTERS_DIR, ["cargo", "fmt", "--check"]),
        (REGISTERS_DIR, ["cargo", "check", "--locked"]),
        (EEPROM_DIR, ["cargo", "fmt", "--check"]),
        (EEPROM_DIR, ["cargo", "check", "--locked"]),
        (SERVO_EEPROM_DIR, ["cargo", "fmt", "--check"]),
        (SERVO_EEPROM_DIR, ["cargo", "check", "--locked"]),
    ],
    "build": [
        (APP_SERVO_CONTROLLER_DIR, ["cargo", "build", "--locked"]),
    ],
    "hardware-build": [
        (
            APP_SERVO_CONTROLLER_DIR,
            [
                "cargo",
                "build",
                "--locked",
                "--examples",
                "--features",
                "hardware-tests",
                "--message-format=json-render-diagnostics",
            ],
        ),
    ],
    "uf2": [
        (APP_SERVO_CONTROLLER_DIR, ["cargo", "run", "--locked"]),
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
                if "--message-format=json-render-diagnostics" in arguments:
                    print_cargo_message(line)
                else:
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


def print_cargo_message(line: str) -> None:
    try:
        message = json.loads(line)
    except json.JSONDecodeError:
        print(line, end="", flush=True)
        return
    if not isinstance(message, dict):
        return
    if message.get("reason") == "compiler-message":
        rendered = message.get("message", {}).get("rendered")
        if rendered:
            print(rendered, end="", flush=True)


def hardware_artifacts(output: str) -> list[Path]:
    artifacts = []
    for line in output.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(message, dict):
            continue
        if message.get("reason") != "compiler-artifact":
            continue
        if "example" not in message.get("target", {}).get("kind", []):
            continue
        executable = message.get("executable")
        if not isinstance(executable, str) or not executable:
            continue
        path = Path(executable)
        if path not in artifacts:
            artifacts.append(path)
    if not artifacts:
        raise ValueError("Cargo reported no hardware-test executables")
    return artifacts


def find_objdump(directory: Path) -> Path:
    """Find LLVM in the active Rust toolchain, including rustup overrides."""
    result = subprocess.run(
        ["rustc", "--print", "sysroot"],
        cwd=directory,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        shell=False,
        check=False,
    )
    if result.returncode != 0:
        raise ValueError(result.stderr.strip() or "Could not locate Rust sysroot")
    sysroot = result.stdout.strip()
    if not sysroot:
        raise ValueError("Rust reported an empty sysroot")
    candidates = [
        path
        for name in ("llvm-objdump", "llvm-objdump.exe")
        for path in (Path(sysroot) / "lib" / "rustlib").glob(f"*/bin/{name}")
        if path.is_file()
    ]
    if len(candidates) != 1:
        raise ValueError(
            "Could not uniquely locate llvm-objdump in the active toolchain. "
            "Run: rustup component add llvm-tools-preview"
        )
    return candidates[0]


def hardware_reports(executable: Path, directory: Path) -> CommandResult:
    """Relink one example with a map and disassemble that exact ELF."""
    executable = (
        executable.resolve()
        if executable.is_absolute()
        else (directory / executable).resolve()
    )
    try:
        objdump = find_objdump(directory)
        # A unique linker output forces Cargo to relink even when a previous
        # map was deleted. Reports replace their final files only on success.
        with tempfile.TemporaryDirectory(
            prefix=".reports-",
            dir=executable.parent,
        ) as temporary:
            temporary = Path(temporary)
            map_file = temporary / "output.map"
            dump_file = temporary / "output.disasm"
            build = run_command(
                [
                    "cargo",
                    "rustc",
                    "--locked",
                    "--example",
                    executable.name,
                    "--features",
                    "hardware-tests",
                    "--message-format=json-render-diagnostics",
                    "--",
                    "-C",
                    f"link-arg=-Map={map_file}",
                ],
                directory,
            )
            if build.return_code != 0:
                return build
            artifacts = hardware_artifacts(build.output)
            resolved = [
                (path if path.is_absolute() else directory / path).resolve()
                for path in artifacts
            ]
            if executable not in resolved:
                raise ValueError("Map build did not report the expected ELF")
            if not map_file.is_file() or map_file.stat().st_size == 0:
                raise ValueError("Linker did not produce a nonempty map file")
            command = [
                str(objdump),
                "--disassemble",
                "--demangle",
                "--section-headers",
                "--syms",
                str(executable),
            ]
            print(
                f"[{directory.name}] {' '.join(command)} -> "
                f"{executable.with_suffix('.disasm')}",
                flush=True,
            )
            with dump_file.open("w", encoding="utf-8", newline="\n") as output:
                result = subprocess.run(
                    command,
                    cwd=directory,
                    stdout=output,
                    stderr=subprocess.PIPE,
                    text=True,
                    encoding="utf-8",
                    errors="replace",
                    shell=False,
                    check=False,
                )
            if result.stderr:
                print(result.stderr, end="", flush=True)
            if result.returncode != 0:
                return CommandResult(result.returncode, result.stderr)
            if dump_file.stat().st_size == 0:
                raise ValueError("llvm-objdump produced an empty disassembly")
            map_file.replace(executable.with_suffix(".map"))
            dump_file.replace(executable.with_suffix(".disasm"))
        return CommandResult(0, "Map and disassembly generated\n")
    except (OSError, ValueError) as error:
        message = f"Hardware reports failed: {error}"
        print(message, flush=True)
        return CommandResult(1, message + "\n")


def test_kind(arguments: list[str]) -> str | None:
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


def write_doc_index() -> CommandResult:
    """Publish an overview only when every expected package page exists."""
    try:
        links = []
        for _, name in DOC_PACKAGES:
            relative = f"{name.replace('-', '_')}/index.html"
            if not (DOC_OUTPUT_DIR / relative).is_file():
                raise ValueError(f"Missing generated documentation: {relative}")
            links.append(f'<li><a href="{relative}">{html.escape(name)}</a></li>')
        page = (
            '<!doctype html>\n<html lang="en"><head><meta charset="utf-8">'
            '<meta name="viewport" content="width=device-width, initial-scale=1">'
            "<title>HexaRoam Rust documentation</title></head><body>"
            "<h1>HexaRoam Rust documentation</h1>"
            "<p>Modules, types, traits and implementations, including private items. "
            "Generated for thumbv6m-none-eabi; hardware-test examples are excluded.</p>"
            "<ul>" + "".join(links) + "</ul></body></html>\n"
        )
        index = DOC_OUTPUT_DIR / "index.html"
        index.write_text(page, encoding="utf-8", newline="\n")
        print(f"Documentation overview: {index}", flush=True)
        return CommandResult(0, str(index))
    except (OSError, ValueError) as error:
        message = f"Documentation overview failed: {error}"
        print(message, flush=True)
        return CommandResult(1, message)


def select_tasks(
    task: str,
    app: str | None,
) -> list[tuple[Path, list[str]]]:
    """Select task commands without executing them."""
    if task not in TASKS:
        raise ValueError(f"Unknown task: {task}")

    applications = {
        "servo-controller": APP_SERVO_CONTROLLER_DIR,
        "calibration-station": APP_CALIBRATION_STATION_DIR,
    }

    if app is not None and app not in applications:
        raise ValueError(f"Unknown application: {app}")

    commands = {
        "build": ["cargo", "build", "--locked"],
        "uf2": ["cargo", "run", "--locked"],
        "hardware-build": [
            "cargo",
            "build",
            "--locked",
            "--examples",
            "--features",
            "hardware-tests",
            "--message-format=json-render-diagnostics",
        ],
    }

    if task not in commands:
        if app is not None:
            raise ValueError(f"Task {task!r} does not support application selection")
        return TASKS[task]

    if app is not None:
        selected = [applications[app]]
    elif task == "hardware-build":
        selected = list(applications.values())
    else:
        selected = [APP_SERVO_CONTROLLER_DIR]

    return [(directory, commands[task].copy()) for directory in selected]


def has_hardware_tests(directory: Path) -> bool:
    """Check for test sources directly inside hardware_tests."""
    return any(
        source.is_file() for source in (directory / "hardware_tests").glob("*.rs")
    )


def run_task(task: str, app: str | None = None) -> int:
    results = []
    skipped_applications = []
    for directory, arguments in select_tasks(task, app):
        if task == "hardware-build" and not has_hardware_tests(directory):
            print(
                f"[SKIP] {directory.name}: no hardware-test sources",
                flush=True,
            )
            skipped_applications.append(directory.name)
            continue

        result = run_command(arguments, directory)
        results.append((directory, arguments, result))

        if task == "hardware-build" and result.return_code == 0:
            try:
                artifacts = hardware_artifacts(result.output)
            except ValueError as error:
                print(str(error), flush=True)
                results.append(
                    (
                        directory,
                        ["hardware-artifact-discovery"],
                        CommandResult(1, str(error)),
                    )
                )
                continue
            for executable in artifacts:
                reports = hardware_reports(executable, directory)
                results.append(
                    (
                        directory,
                        ["hardware-reports", str(executable)],
                        reports,
                    )
                )
                if reports.return_code != 0:
                    continue
                conversion = [
                    "elf2uf2-rs",
                    str(executable),
                    str(executable.with_suffix(".uf2")),
                ]
                converted = run_command(conversion, directory)
                results.append((directory, conversion, converted))

    if task == "doc" and all(result.return_code == 0 for _, _, result in results):
        results.append((PROJECT_ROOT, ["documentation-index"], write_doc_index()))

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

    for name in skipped_applications:
        print(
            f"[SKIP] {name}: no hardware-test sources",
            flush=True,
        )

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
    parser.add_argument(
        "--app",
        choices=("servo-controller", "calibration-station"),
        help=(
            "Application for build, uf2 or hardware-build. "
            "Defaults to servo-controller for build/uf2 "
            "and both applications for hardware-build."
        ),
    )
    args = parser.parse_args()

    try:
        select_tasks(args.task, args.app)
    except ValueError as error:
        parser.error(str(error))

    return run_task(args.task, args.app)


if __name__ == "__main__":
    raise SystemExit(main())
