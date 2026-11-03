"""Run the documented UART register scenarios against real hardware."""

import argparse
import json
import struct
import sys
import time
import zlib
from dataclasses import dataclass
from pathlib import Path

from dev import color_status

PROJECT_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SUITE_DIR = (
    PROJECT_ROOT / "firmware" / "pico" / "hardware_tests" / "suites"
)


def resolve_suite(value: str) -> Path:
    """Resolve bare names in the suite directory and explicit paths from cwd."""
    path = Path(value).expanduser()

    if not path.is_absolute() and "/" not in value and "\\" not in value:
        if not path.suffix:
            path = path.with_suffix(".json")
        path = DEFAULT_SUITE_DIR / path

    if path.suffix.lower() != ".json":
        raise ValueError("Suite must be a JSON file")

    return path.resolve()


@dataclass(frozen=True)
class Step:
    action: str
    transmit: bytes
    expected: bytes | None
    observe_seconds: float


@dataclass(frozen=True)
class Case:
    identifier: str
    title: str
    steps: list[Step]


class StepFailure(RuntimeError):
    pass


def with_crc(body: bytes) -> bytes:
    return body + struct.pack("<I", zlib.crc32(body))


def hex_bytes(data: bytes) -> str:
    return data.hex(" ").upper() if data else "<empty>"


def load_step(raw: dict) -> Step:
    action = raw["action"]
    body = bytes.fromhex(raw["request_body_hex"])
    encoding = raw["encoding"]

    if encoding == "raw":
        transmit = body
    elif encoding == "crc":
        transmit = with_crc(body)
    elif encoding == "bad_crc":
        transmit = with_crc(body)
        transmit = transmit[:-1] + bytes([transmit[-1] ^ 0x80])
    else:
        raise ValueError(f"{action}: unknown encoding {encoding!r}")

    if not transmit:
        raise ValueError(f"{action}: empty transmission")

    if transmit != bytes.fromhex(raw["tx_hex"]):
        raise ValueError(f"{action}: stored TX bytes do not match generation")

    response_body = raw["response_body_hex"]
    expected = None if response_body is None else with_crc(bytes.fromhex(response_body))

    stored_response = raw["expected_rx_hex"]
    stored_expected = (
        None if stored_response is None else bytes.fromhex(stored_response)
    )

    if expected != stored_expected:
        raise ValueError(f"{action}: stored RX bytes do not match generation")

    observe_ms = raw["observe_ms"]
    if type(observe_ms) is not int or observe_ms <= 0:
        raise ValueError(f"{action}: observe_ms must be a positive integer")

    return Step(action, transmit, expected, observe_ms / 1000)


def load_suite(path: Path) -> tuple[dict, list[Step], list[Case]]:
    data = json.loads(path.read_text(encoding="utf-8"))

    if data["schema_version"] != 1:
        raise ValueError("Unsupported fixture schema")

    settings = data["serial"]
    if (
        settings["data_bits"] != 8
        or settings["parity"] != "none"
        or settings["stop_bits"] != 1
        or settings["flow_control"] != "none"
    ):
        raise ValueError("This runner currently supports 8N1 without flow control")

    baseline = [load_step(item) for item in data["before_each_case"]]
    cases = [
        Case(
            item["id"],
            item["title"],
            [load_step(step) for step in item["steps"]],
        )
        for item in data["cases"]
    ]

    identifiers = [case.identifier for case in cases]
    if len(set(identifiers)) != len(identifiers):
        raise ValueError("Duplicate scenario IDs")

    if not baseline or not cases or any(not case.steps for case in cases):
        raise ValueError("Baseline and scenarios must contain steps")

    return data, baseline, cases


def fixture_checks(data: dict) -> list[Step]:
    """Check the expected firmware state before sending modifying commands."""
    timeout = data["timing"]["read_response_timeout_ms"] / 1000
    if timeout <= 0:
        raise ValueError("Fixture response timeout must be positive")

    checks = []
    for name, index, format_string in [
        ("protocol_version", 0x00, "<H"),
        ("status", 0x01, "<B"),
        ("servo_count", 0x04, "<B"),
    ]:
        payload = struct.pack(format_string, data["fixture"][name])
        header = struct.pack("<BBH", 0x01, index, len(payload))
        checks.append(
            Step(
                f"Check fixture {name}",
                with_crc(header),
                with_crc(header + payload),
                timeout,
            )
        )

    return checks


def read_available(port) -> bytes:
    # timeout=0 makes even the one-byte fallback nonblocking.
    return port.read(min(4096, max(1, port.in_waiting)))


def failure(action: str, reason: str, expected: bytes | None, actual: bytes):
    expectation = "<no response>" if expected is None else hex_bytes(expected)
    return StepFailure(
        f"{action}: {reason}\n"
        f"       Expected: {expectation}\n"
        f"       Received: {hex_bytes(actual)}"
    )


def execute_step(port, step: Step) -> None:
    # Do not silently discard late responses from an earlier operation.
    pending = read_available(port)
    if pending:
        raise failure(
            step.action,
            "unexpected input before transmission",
            None,
            pending,
        )

    written = port.write(step.transmit)
    if written != len(step.transmit):
        raise StepFailure(
            f"{step.action}: partial write ({written} of {len(step.transmit)} bytes)"
        )

    received = bytearray()
    deadline = time.monotonic() + step.observe_seconds

    while True:
        chunk = read_available(port)

        if chunk:
            received.extend(chunk)

            if step.expected is None:
                raise failure(
                    step.action,
                    "a response was not permitted",
                    None,
                    bytes(received),
                )

            if not step.expected.startswith(received):
                raise failure(
                    step.action,
                    "response differs or contains extra bytes",
                    step.expected,
                    bytes(received),
                )

        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break

        # Observe the entire window, even after a complete correct reply,
        # so additional or delayed bytes are also detected.
        time.sleep(min(0.002, remaining))

    if step.expected is not None and bytes(received) != step.expected:
        raise failure(
            step.action,
            "response timeout or incomplete response",
            step.expected,
            bytes(received),
        )


def print_summary(passed: int, failed: int, total: int, success: bool) -> None:
    pending = total - passed - failed
    print(f"\nTotal: {passed} passed, {failed} failed, {pending} not completed")
    status = "PASSED" if success else "FAILED"
    print(f"Overall: {color_status(status, success)}")


def run_suite(port, checks: list[Step], baseline: list[Step], cases: list[Case]) -> int:
    passed = 0

    try:
        for step in checks:
            execute_step(port, step)
    except (StepFailure, OSError) as error:
        print(f"[{color_status('FAIL', False)}] Fixture check\n       {error}")
        print_summary(0, 0, len(cases), False)
        return 1

    print(f"[{color_status('PASS', True)}] Fixture check")

    for case in cases:
        try:
            for step in baseline:
                execute_step(port, step)

            for step in case.steps:
                execute_step(port, step)
        except (StepFailure, OSError) as error:
            print(
                f"[{color_status('FAIL', False)}] "
                f"{case.identifier}: {case.title}\n       {error}"
            )
            print_summary(passed, 1, len(cases), False)
            return 1

        passed += 1
        print(f"[{color_status('PASS', True)}] {case.identifier}: {case.title}")

    print_summary(passed, 0, len(cases), True)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", required=True, help="COM5 or /dev/ttyUSB0")
    parser.add_argument(
        "--suite", required=True, help="Suite name or explicit JSON path"
    )
    parser.add_argument(
        "--case",
        action="append",
        default=[],
        help="Run one scenario ID; repeat to select several",
    )
    args = parser.parse_args()

    try:
        suite_path = resolve_suite(args.suite)
        data, baseline, cases = load_suite(suite_path)
        checks = fixture_checks(data)

        if args.case:
            requested = set(args.case)
            unknown = requested - {case.identifier for case in cases}
            if unknown:
                raise ValueError("Unknown scenario IDs: " + ", ".join(sorted(unknown)))
            cases = [case for case in cases if case.identifier in requested]

        # Keep pySerial optional for the ordinary development tools.
        try:
            import serial
        except ImportError:
            print(
                'pySerial is missing. Run: python -m pip install -e ".[hardware]"',
                file=sys.stderr,
            )
            return 2

        print(f"Port: {args.port}")
        print(f"Suite: {suite_path}")
        print(f"Required firmware: {data['firmware']}")
        print(f"Scenarios: {len(cases)}")

        with serial.Serial(
            port=args.port,
            baudrate=data["serial"]["baud"],
            bytesize=serial.EIGHTBITS,
            parity=serial.PARITY_NONE,
            stopbits=serial.STOPBITS_ONE,
            timeout=0,
            write_timeout=1,
            xonxoff=False,
            rtscts=False,
            dsrdtr=False,
        ) as port:
            return run_suite(port, checks, baseline, cases)

    except KeyboardInterrupt:
        print("\nInterrupted.")
        return 130
    except (OSError, ValueError, KeyError, TypeError, struct.error) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
