"""Interactive calibration console and exact angle conversion."""

import argparse
import re
import sys
from pathlib import Path

from .calibration_client import CalibrationClient
from .calibration_data import I32_MAX, I32_MIN, CalibrationData, load, save
from .calibration_session import CalibrationSession
from .wire_protocol import NackError, ProtocolError

ANGLE_PATTERN = re.compile(r"([+-]?)([0-9]+)(?:[.,]([0-9]{1,2}))?")


def parse_angle(text: str) -> int:
    """Convert decimal degrees into signed integer centidegrees."""
    match = ANGLE_PATTERN.fullmatch(text.strip())

    if match is None:
        raise ValueError(
            "Enter degrees with at most two decimal places, for example -45.25"
        )

    sign, whole, fraction = match.groups()
    fractional_part = int((fraction or "").ljust(2, "0"))
    centidegrees = int(whole) * 100 + fractional_part

    if sign == "-":
        centidegrees = -centidegrees

    if not I32_MIN <= centidegrees <= I32_MAX:
        raise ValueError("Angle exceeds the signed 32-bit range")

    return centidegrees


def format_angle(centidegrees: int) -> str:
    """Format signed integer centidegrees with two decimal places."""
    if type(centidegrees) is not int or not I32_MIN <= centidegrees <= I32_MAX:
        raise ValueError("Angle must be a signed 32-bit integer")

    sign = "-" if centidegrees < 0 else ""
    magnitude = abs(centidegrees)

    return f"{sign}{magnitude // 100}.{magnitude % 100:02d}"


HELP = """Commands:
  status          Show station status
  pulse 1500      Set pulse width in microseconds
  enable          Start the prepared output
  disable         Stop output
  lower -45.00    Record measured lower angle at current accepted pulse
  upper 45.00     Record measured upper angle at current accepted pulse
  save            Validate points, stop output and save this servo
  help            Show commands
  quit            Stop output and exit
"""

COMMAND_ARGUMENTS = {
    "status": 0,
    "pulse": 1,
    "enable": 0,
    "disable": 0,
    "lower": 1,
    "upper": 1,
    "save": 0,
    "help": 0,
    "quit": 0,
}


def show_status(client: CalibrationClient) -> None:
    status = client.get_status()
    pulse = f"{status.pulse_width_us} us" if status.pulse_ready else "not prepared"
    print(
        f"Status={status.flags:#010x}, pulse={pulse}, "
        f"running={status.running}, output_error={status.output_error}"
    )


def run_console(
    client: CalibrationClient,
    session: CalibrationSession,
    path: Path,
) -> None:
    dirty = False
    print(HELP)

    while True:
        try:
            line = input("calibration> ")
        except EOFError:
            if dirty:
                print("Unsaved measurements were not saved.")
            return

        parts = line.split()
        if not parts:
            continue

        command, *arguments = parts

        try:
            expected = COMMAND_ARGUMENTS.get(command)
            if expected is None or len(arguments) != expected:
                raise ValueError("Unknown command or incorrect arguments. Use help.")

            if command == "help":
                print(HELP)

            elif command == "status":
                show_status(client)

            elif command == "pulse":
                pulse_width_us = int(arguments[0])
                client.set_pulse(pulse_width_us)
                print(f"Accepted pulse: {pulse_width_us} us")

            elif command == "enable":
                client.set_enabled(True)
                show_status(client)

            elif command == "disable":
                client.set_enabled(False)
                show_status(client)

            elif command in ("lower", "upper"):
                angle = parse_angle(arguments[0])
                status = client.get_status()

                if not status.pulse_ready or not status.running or status.output_error:
                    raise ValueError(
                        "Capture requires running output with a prepared "
                        "pulse and no output fault"
                    )

                record = (
                    session.record_lower if command == "lower" else session.record_upper
                )
                record(angle, status.pulse_width_us)
                dirty = True

                print(
                    f"Recorded {command}: {format_angle(angle)} deg "
                    f"at {status.pulse_width_us} us"
                )

            elif command == "save":
                # Validate before changing hardware or writing the file.
                updated = session.finish()

                client.set_enabled(False)
                if client.get_status().running:
                    raise RuntimeError("Output stop was not confirmed")

                save(path, updated)
                dirty = False
                print(f"Saved measurements to {path}; output inactive")

            elif command == "quit":
                if dirty:
                    answer = input("Discard unsaved measurements and quit? [y/N] ")
                    if answer.strip().lower() not in ("y", "yes"):
                        continue
                return

        except NackError as error:
            # A valid rejection leaves the connection usable.
            print(f"Rejected: {error}")

        except ValueError as error:
            # Invalid input preserves previously captured points.
            print(f"Error: {error}")

        except OSError as error:
            if not client.usable:
                raise
            # File errors leave the captured points available for retry.
            print(f"File error: {error}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Capture two-point servo calibration via UART.",
    )
    parser.add_argument("--port", required=True, help="COM8 or /dev/ttyUSB0")
    parser.add_argument("--file", type=Path, required=True)
    parser.add_argument("--servo-id", type=int, required=True)
    parser.add_argument(
        "--servo-count",
        type=int,
        help="Required for a new document; must match an existing document",
    )
    args = parser.parse_args()

    try:
        if args.file.exists():
            data = load(args.file)
            if args.servo_count is not None and args.servo_count != data.servo_count:
                raise ValueError("--servo-count differs from the existing document")
        else:
            if args.servo_count is None:
                raise ValueError("--servo-count is required for a new document")
            data = CalibrationData(args.servo_count, ())

        session = CalibrationSession(data, args.servo_id)
    except (OSError, ValueError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2

    client: CalibrationClient | None = None
    result = 0

    try:
        client = CalibrationClient(args.port)
        client.set_enabled(False)

        status = client.get_status()
        if status.running or status.output_error:
            raise RuntimeError("Station must be inactive and free of output faults")

        print(f"Port: {args.port}")
        print(f"Document: {args.file}")
        print(f"Servo ID: {args.servo_id}")
        print("Physical station output: GP0")
        show_status(client)

        run_console(client, session, args.file)

    except KeyboardInterrupt:
        print("\nInterrupted; unsaved measurements were not saved.")
        result = 130

    except EOFError:
        print("\nUnsaved measurements were not saved.")

    except (OSError, ProtocolError, RuntimeError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        result = 1

    finally:
        if client is not None:
            if client.usable:
                try:
                    client.set_enabled(False)
                except (OSError, ProtocolError) as error:
                    print(
                        f"ERROR: Output stop failed: {error}",
                        file=sys.stderr,
                    )
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
                print(f"ERROR: Closing port failed: {error}", file=sys.stderr)
                if result == 0:
                    result = 1

    return result


if __name__ == "__main__":
    raise SystemExit(main())
