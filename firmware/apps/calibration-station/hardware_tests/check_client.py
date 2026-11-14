"""Manual calibration-client check against a real Pico; keep GP0 unloaded."""

import argparse
import sys
import time

from hexaroam_tools.calibration_client import CalibrationClient
from hexaroam_tools.wire_protocol import NackError, ProtocolError


def check_status(
    client: CalibrationClient,
    expected_flags: int,
    expected_pulse: int,
) -> None:
    status = client.get_status()

    if status.flags != expected_flags or status.pulse_width_us != expected_pulse:
        raise RuntimeError(
            f"Unexpected status: flags={status.flags:#010x}, "
            f"pulse={status.pulse_width_us}; "
            f"expected flags={expected_flags:#010x}, pulse={expected_pulse}"
        )


def run_checks(client: CalibrationClient) -> None:
    client.set_enabled(False)
    print("[PASS] Output disabled")

    client.set_pulse(1500)
    check_status(client, 1, 1500)
    print("[PASS] 1500 us prepared; output inactive")

    client.set_enabled(True)
    check_status(client, 3, 1500)
    print("[PASS] Prepared output enabled")

    # Allow more than two output periods before testing an active update.
    time.sleep(0.05)

    client.set_pulse(1700)
    check_status(client, 3, 1700)
    print("[PASS] Active pulse updated to 1700 us")

    try:
        client.set_pulse(2001)
    except NackError as error:
        if error.command != 0x03 or error.code != 0x03:
            raise RuntimeError(
                f"Unexpected NACK: command={error.command:#04x}, code={error.code:#04x}"
            ) from error
    else:
        raise RuntimeError("Expected InvalidValue NACK for 2001 us")

    if not client.usable:
        raise RuntimeError("Valid NACK incorrectly disabled the connection")

    check_status(client, 3, 1700)
    print("[PASS] Invalid pulse rejected; value and connection preserved")

    client.set_enabled(False)
    check_status(client, 1, 1700)
    print("[PASS] Output stopped; 1700 us retained")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", required=True, help="COM8 or /dev/ttyUSB0")
    args = parser.parse_args()

    client: CalibrationClient | None = None
    result = 0

    try:
        client = CalibrationClient(args.port)
        run_checks(client)
    except KeyboardInterrupt:
        print("\nInterrupted.", file=sys.stderr)
        result = 130
    except (OSError, ProtocolError, RuntimeError, ValueError) as error:
        print(f"[FAIL] {error}", file=sys.stderr)
        result = 1
    finally:
        if client is not None:
            if client.usable:
                try:
                    client.set_enabled(False)
                except (OSError, ProtocolError) as error:
                    print(f"[FAIL] Cleanup stop failed: {error}", file=sys.stderr)
                    if result == 0:
                        result = 1
            else:
                print(
                    "Output stop cannot be confirmed; reset the Pico.",
                    file=sys.stderr,
                )

            try:
                client.close()
            except OSError as error:
                print(f"[FAIL] Closing port failed: {error}", file=sys.stderr)
                if result == 0:
                    result = 1

    print("Overall: PASSED" if result == 0 else "Overall: FAILED")
    return result


if __name__ == "__main__":
    raise SystemExit(main())
