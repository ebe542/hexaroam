"""Capture two calibration points without file or hardware access."""

from dataclasses import dataclass

from .calibration_data import (
    I32_MAX,
    I32_MIN,
    U32_MAX,
    CalibrationData,
    ServoCalibration,
)


@dataclass(frozen=True)
class MeasurementPoint:
    angle_centidegrees: int
    pulse_width_us: int

    def __post_init__(self) -> None:
        if (
            type(self.angle_centidegrees) is not int
            or not I32_MIN <= self.angle_centidegrees <= I32_MAX
        ):
            raise ValueError("Angle must be a signed 32-bit integer")

        if (
            type(self.pulse_width_us) is not int
            or not 1 <= self.pulse_width_us <= U32_MAX
        ):
            raise ValueError("Pulse width must be a positive unsigned 32-bit integer")


class CalibrationSession:
    def __init__(self, data: CalibrationData, servo_id: int) -> None:
        if type(servo_id) is not int or not 0 <= servo_id < data.servo_count:
            raise ValueError("Servo ID must be within the configured range")

        self._data = data
        self._servo_id = servo_id

        # Always capture fresh points, including when replacing a servo.
        self._lower_point: MeasurementPoint | None = None
        self._upper_point: MeasurementPoint | None = None

    @property
    def lower_point(self) -> MeasurementPoint | None:
        return self._lower_point

    @property
    def upper_point(self) -> MeasurementPoint | None:
        return self._upper_point

    def record_lower(self, angle_centidegrees: int, pulse_width_us: int) -> None:
        point = MeasurementPoint(angle_centidegrees, pulse_width_us)

        if self._upper_point is not None:
            self._make_calibration(point, self._upper_point)

        # Commit only after validating the prospective pair.
        self._lower_point = point

    def record_upper(self, angle_centidegrees: int, pulse_width_us: int) -> None:
        point = MeasurementPoint(angle_centidegrees, pulse_width_us)

        if self._lower_point is not None:
            self._make_calibration(self._lower_point, point)

        self._upper_point = point

    def finish(self) -> CalibrationData:
        """Return updated data only after both points are valid."""
        if self._lower_point is None or self._upper_point is None:
            raise ValueError("Both measurement points are required")

        calibration = self._make_calibration(
            self._lower_point,
            self._upper_point,
        )

        # Preserve other entries and their order without changing the source.
        entries = list(self._data.servos)

        for index, existing in enumerate(entries):
            if existing.servo_id == self._servo_id:
                entries[index] = calibration
                break
        else:
            entries.append(calibration)

        return CalibrationData(
            servo_count=self._data.servo_count,
            servos=tuple(entries),
        )

    def _make_calibration(
        self,
        lower: MeasurementPoint,
        upper: MeasurementPoint,
    ) -> ServoCalibration:
        # Reuse the data model's angle-range and pulse-pair validation.
        return ServoCalibration(
            servo_id=self._servo_id,
            lower_angle_centidegrees=lower.angle_centidegrees,
            lower_pulse_us=lower.pulse_width_us,
            upper_angle_centidegrees=upper.angle_centidegrees,
            upper_pulse_us=upper.pulse_width_us,
        )
