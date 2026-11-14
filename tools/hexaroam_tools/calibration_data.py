"""Portable calibration documents, independent of hardware and transport."""

import json
from dataclasses import asdict, dataclass
from pathlib import Path

SCHEMA_VERSION = 1

U32_MAX = 2**32 - 1
I32_MIN = -(2**31)
I32_MAX = 2**31 - 1


def _require_integer(
    name: str,
    value: object,
    minimum: int,
    maximum: int,
) -> None:
    # bool is a subclass of int, but is not a valid calibration value.
    if type(value) is not int:
        raise ValueError(f"{name} must be an integer")

    if not minimum <= value <= maximum:
        raise ValueError(f"{name} must be between {minimum} and {maximum}")


def _require_fields(
    value: object,
    expected: set[str],
    name: str,
) -> None:
    if not isinstance(value, dict):
        raise ValueError(f"{name} must be an object")

    if set(value) != expected:
        raise ValueError(f"{name} must contain exactly: {', '.join(sorted(expected))}")


@dataclass(frozen=True)
class ServoCalibration:
    servo_id: int
    lower_angle_centidegrees: int
    lower_pulse_us: int
    upper_angle_centidegrees: int
    upper_pulse_us: int

    def __post_init__(self) -> None:
        _require_integer("servo_id", self.servo_id, 0, U32_MAX)

        for name, value in (
            ("lower_angle_centidegrees", self.lower_angle_centidegrees),
            ("upper_angle_centidegrees", self.upper_angle_centidegrees),
        ):
            _require_integer(name, value, I32_MIN, I32_MAX)

        for name, value in (
            ("lower_pulse_us", self.lower_pulse_us),
            ("upper_pulse_us", self.upper_pulse_us),
        ):
            _require_integer(name, value, 1, U32_MAX)

        if self.lower_angle_centidegrees >= self.upper_angle_centidegrees:
            raise ValueError("Lower angle must be smaller than upper angle")

        if self.lower_pulse_us == self.upper_pulse_us:
            raise ValueError("Pulse widths must differ")


@dataclass(frozen=True)
class CalibrationData:
    servo_count: int
    servos: tuple[ServoCalibration, ...]

    def __post_init__(self) -> None:
        _require_integer("servo_count", self.servo_count, 1, U32_MAX)

        if not isinstance(self.servos, tuple):
            raise ValueError("servos must be an immutable tuple")

        seen: set[int] = set()

        for calibration in self.servos:
            if not isinstance(calibration, ServoCalibration):
                raise ValueError("Each servo must be a ServoCalibration")

            servo_id = calibration.servo_id

            if servo_id >= self.servo_count:
                raise ValueError(f"Servo ID {servo_id} exceeds configured servo count")

            if servo_id in seen:
                raise ValueError(f"Duplicate servo ID: {servo_id}")

            seen.add(servo_id)

    def require_complete(self) -> None:
        """Reject incomplete data before export or EEPROM programming."""
        # Construction already guarantees unique IDs within the range.
        if len(self.servos) != self.servo_count:
            raise ValueError(
                f"Calibration is incomplete: "
                f"{len(self.servos)} of {self.servo_count} servos"
            )


def from_json(text: str) -> CalibrationData:
    raw = json.loads(text)

    _require_fields(
        raw,
        {"schema_version", "servo_count", "servos"},
        "Calibration document",
    )

    _require_integer(
        "schema_version",
        raw["schema_version"],
        SCHEMA_VERSION,
        SCHEMA_VERSION,
    )

    entries = raw["servos"]

    if not isinstance(entries, list):
        raise ValueError("servos must be a JSON array")

    calibrations: list[ServoCalibration] = []

    for entry in entries:
        _require_fields(
            entry,
            {
                "servo_id",
                "lower_angle_centidegrees",
                "lower_pulse_us",
                "upper_angle_centidegrees",
                "upper_pulse_us",
            },
            "Servo calibration",
        )
        calibrations.append(ServoCalibration(**entry))

    return CalibrationData(
        servo_count=raw["servo_count"],
        servos=tuple(calibrations),
    )


def to_json(data: CalibrationData) -> str:
    raw = {
        "schema_version": SCHEMA_VERSION,
        "servo_count": data.servo_count,
        "servos": [asdict(calibration) for calibration in data.servos],
    }

    return json.dumps(raw, indent=2, ensure_ascii=False) + "\n"


def load(path: Path) -> CalibrationData:
    return from_json(path.read_text(encoding="utf-8"))


def save(path: Path, data: CalibrationData) -> None:
    """Save a document, creating missing parent directories."""
    text = to_json(data)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
