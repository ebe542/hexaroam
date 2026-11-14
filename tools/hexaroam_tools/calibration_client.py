"""Synchronous UART client for the single-servo calibration station."""

import math
import struct
from dataclasses import dataclass
from types import TracebackType

from .wire_protocol import NackError, ProtocolError, exchange, frame

PULSE_READY = 1 << 0
RUNNING = 1 << 1
OUTPUT_ERROR = 1 << 8


@dataclass(frozen=True)
class StationStatus:
    flags: int
    pulse_width_us: int

    @property
    def pulse_ready(self) -> bool:
        return bool(self.flags & PULSE_READY)

    @property
    def running(self) -> bool:
        return bool(self.flags & RUNNING)

    @property
    def output_error(self) -> bool:
        return bool(self.flags & OUTPUT_ERROR)


class CalibrationClient:
    def __init__(self, port: str, timeout: float = 1.0) -> None:
        if (
            isinstance(timeout, bool)
            or not isinstance(timeout, (int, float))
            or not math.isfinite(timeout)
            or timeout <= 0
        ):
            raise ValueError("Timeout must be positive and finite")

        # Keep pySerial optional for ordinary development-tool tests.
        try:
            import serial
        except ImportError as error:
            raise RuntimeError(
                'Install hardware dependencies: python -m pip install -e ".[hardware]"'
            ) from error

        self._timeout = float(timeout)
        self._port = serial.Serial(
            port=port,
            baudrate=115200,
            bytesize=serial.EIGHTBITS,
            parity=serial.PARITY_NONE,
            stopbits=serial.STOPBITS_ONE,
            timeout=min(0.01, self._timeout),
            write_timeout=self._timeout,
            xonxoff=False,
            rtscts=False,
            dsrdtr=False,
        )
        self._usable = True

    @property
    def usable(self) -> bool:
        return self._usable

    def get_status(self) -> StationStatus:
        payload = self._request(0x01, result_length=8)
        flags, pulse = struct.unpack("<II", payload)
        return StationStatus(flags, pulse)

    def set_pulse(self, pulse_width_us: int) -> None:
        if type(pulse_width_us) is not int or not 1 <= pulse_width_us <= 2**32 - 1:
            raise ValueError("Pulse width must be a positive u32 integer")

        self._request(0x03, struct.pack("<I", pulse_width_us))

    def set_enabled(self, enabled: bool) -> None:
        if type(enabled) is not bool:
            raise ValueError("Enabled must be a boolean")

        self._request(0x02, bytes([int(enabled)]))

    def _request(
        self,
        command: int,
        parameters: bytes = b"",
        result_length: int = 0,
    ) -> bytes:
        if not self._usable:
            raise ConnectionError("Calibration connection is closed or unusable")

        request = frame(bytes([command]) + parameters)

        try:
            payload, _ = exchange(
                self._port,
                request,
                command,
                result_length,
                self._timeout,
            )
        except NackError:
            # A CRC-checked NACK completes the transaction normally.
            raise
        except (ProtocolError, OSError):
            # Includes receive/write timeouts. Do not reuse the connection:
            # a delayed response could be mistaken for a later request.
            self._usable = False
            try:
                self._port.close()
            except OSError:
                # Preserve the original communication error.
                pass
            raise

        return payload

    def close(self) -> None:
        """Close communication; this does not send an output-stop command."""
        self._usable = False
        self._port.close()

    def __enter__(self) -> "CalibrationClient":  # noqa: PYI034
        return self

    def __exit__(
        self,
        _exception_type: type[BaseException] | None,
        _exception: BaseException | None,
        _traceback: TracebackType | None,
    ) -> None:
        self.close()
