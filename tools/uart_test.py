"""Run documented UART hardware scenarios with register or COBS framing."""

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
    PROJECT_ROOT
    / "firmware"
    / "apps"
    / "servo-controller"
    / "hardware_tests"
    / "suites"
)

# Detect trailing bytes briefly after a complete response without waiting
# for the full response timeout on every successful step.
RESPONSE_SETTLE_SECONDS = 0.020


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
    retry_response: bytes | None = None
    max_attempts: int = 1
    retry_delay_seconds: float = 0.0


@dataclass(frozen=True)
class Case:
    identifier: str
    title: str
    steps: list[Step]


class StepFailure(RuntimeError):
    pass


def with_crc(body: bytes) -> bytes:
    return body + struct.pack("<I", zlib.crc32(body))


def cobs_encode(data: bytes) -> bytes:
    """Encode without delimiters, matching the Rust encoder."""
    output = bytearray([0])
    code_index = 0
    code = 1

    for byte in data:
        if byte == 0:
            output[code_index] = code
            code_index = len(output)
            output.append(0)
            code = 1
        else:
            output.append(byte)
            code += 1

            if code == 255:
                output[code_index] = code
                code_index = len(output)
                output.append(0)
                code = 1

    output[code_index] = code
    return bytes(output)


def encode_wire(
    body: bytes,
    framing: str,
    bad_crc: bool = False,
) -> bytes:
    """Add CRC and optionally COBS with both frame delimiters."""
    if framing not in ("register_crc32", "cobs_crc32"):
        raise ValueError(f"Unknown framing: {framing!r}")

    if framing == "cobs_crc32" and not 1 <= len(body) <= 250:
        raise ValueError("COBS content must contain 1..250 bytes")

    encoded = with_crc(body)

    if bad_crc:
        encoded = encoded[:-1] + bytes([encoded[-1] ^ 0x80])

    if framing == "cobs_crc32":
        encoded = b"\x00" + cobs_encode(encoded) + b"\x00"

    return encoded


def hex_bytes(data: bytes) -> str:
    return data.hex(" ").upper() if data else "<empty>"


def load_step(raw: dict, framing: str = "register_crc32") -> Step:
    action = raw["action"]
    body = bytes.fromhex(raw["request_body_hex"])
    encoding = raw["encoding"]

    if encoding == "raw":
        transmit = body
    elif encoding == "crc":
        transmit = encode_wire(body, framing)
    elif encoding == "bad_crc":
        transmit = encode_wire(body, framing, bad_crc=True)
    else:
        raise ValueError(f"{action}: unknown encoding {encoding!r}")

    if not transmit:
        raise ValueError(f"{action}: empty transmission")

    if transmit != bytes.fromhex(raw["tx_hex"]):
        raise ValueError(f"{action}: stored TX bytes do not match generation")

    response_body = raw["response_body_hex"]
    expected = (
        None
        if response_body is None
        else encode_wire(bytes.fromhex(response_body), framing)
    )

    stored_response = raw["expected_rx_hex"]
    stored_expected = (
        None if stored_response is None else bytes.fromhex(stored_response)
    )

    if expected != stored_expected:
        raise ValueError(f"{action}: stored RX bytes do not match generation")

    observe_ms = raw["observe_ms"]
    if type(observe_ms) is not int or observe_ms <= 0:
        raise ValueError(f"{action}: observe_ms must be a positive integer")

    retry_hex = raw.get("retry_rx_hex")
    retry_response = None if retry_hex is None else bytes.fromhex(retry_hex)
    max_attempts = raw.get("max_attempts", 1)
    retry_delay_ms = raw.get("retry_delay_ms", 0)

    if type(max_attempts) is not int or max_attempts < 1:
        raise ValueError(f"{action}: max_attempts must be a positive integer")
    if type(retry_delay_ms) is not int or retry_delay_ms < 0:
        raise ValueError(f"{action}: retry_delay_ms must be a nonnegative integer")

    if retry_response is None:
        if max_attempts != 1 or retry_delay_ms != 0:
            raise ValueError(f"{action}: retry options require retry_rx_hex")
    elif (
        expected is None
        or not retry_response
        or retry_response == expected
        or max_attempts < 2
    ):
        raise ValueError(
            f"{action}: retries require a distinct nonempty intermediate reply, "
            "an expected final reply and at least two attempts"
        )

    return Step(
        action,
        transmit,
        expected,
        observe_ms / 1000,
        retry_response,
        max_attempts,
        retry_delay_ms / 1000,
    )


def load_suite(path: Path) -> tuple[dict, list[Step], list[Case]]:
    data = json.loads(path.read_text(encoding="utf-8"))

    if data["schema_version"] != 1:
        raise ValueError("Unsupported fixture schema")

    # Existing suites retain their register framing when this is omitted.
    framing = data.get("framing", "register_crc32")
    if framing not in ("register_crc32", "cobs_crc32"):
        raise ValueError(f"Unknown framing: {framing!r}")

    settings = data["serial"]
    if (
        settings["data_bits"] != 8
        or settings["parity"] != "none"
        or settings["stop_bits"] != 1
        or settings["flow_control"] != "none"
    ):
        raise ValueError("This runner currently supports 8N1 without flow control")

    baseline = [load_step(item, framing) for item in data["before_each_case"]]
    cases = [
        Case(
            item["id"],
            item["title"],
            [load_step(step, framing) for step in item["steps"]],
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

    if data.get("framing", "register_crc32") == "cobs_crc32":
        status = struct.pack("<I", data["fixture"]["status"])
        return [
            Step(
                "Check fixture status",
                encode_wire(b"\x01", "cobs_crc32"),
                encode_wire(b"\x80\x01" + status, "cobs_crc32"),
                timeout,
            )
        ]

    checks = []
    for name, index, format_string in [
        ("protocol_version", 0x00, "<H"),
        ("status", 0x01, "<I"),  # Status is a four-byte little-endian register.
        ("servo_count", 0x04, "<B"),
    ]:
        payload = struct.pack(format_string, data["fixture"][name])
        header = struct.pack("<BBH", 0x01, index, len(payload))
        response_header = struct.pack("<BBH", 0x80, index, len(payload))
        checks.append(
            Step(
                f"Check fixture {name}",
                with_crc(header),
                with_crc(response_header + payload),
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
    """Repeat only after an explicitly permitted complete intermediate reply."""
    for attempt in range(1, step.max_attempts + 1):
        if execute_attempt(port, step):
            return

        assert step.retry_response is not None

        if attempt == step.max_attempts:
            raise failure(
                step.action,
                f"final response not reached after {step.max_attempts} attempts",
                step.expected,
                step.retry_response,
            )

        if step.retry_delay_seconds:
            time.sleep(step.retry_delay_seconds)


def execute_attempt(port, step: Step) -> bool:
    """Return True for final success, False for a permitted retry response."""
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

    replies = tuple(
        reply for reply in (step.expected, step.retry_response) if reply is not None
    )
    received = bytearray()
    deadline = time.monotonic() + step.observe_seconds
    completion_deadline: float | None = None

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

            if not any(reply.startswith(received) for reply in replies):
                raise failure(
                    step.action,
                    "response differs or contains extra bytes",
                    step.expected,
                    bytes(received),
                )

        if any(received == reply for reply in replies) and completion_deadline is None:
            # Allow the full settling interval even when the response
            # completes close to its timeout. Extra bytes still fail above.
            completion_deadline = time.monotonic() + RESPONSE_SETTLE_SECONDS

        active_deadline = (
            deadline if completion_deadline is None else completion_deadline
        )
        remaining = active_deadline - time.monotonic()
        if remaining <= 0:
            break

        # No-reply steps retain their complete observation window.
        # Expected replies use a timeout followed by a short settling window.
        time.sleep(min(0.002, remaining))

    if step.expected is None or bytes(received) == step.expected:
        return True
    if step.retry_response is not None and bytes(received) == step.retry_response:
        return False

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
