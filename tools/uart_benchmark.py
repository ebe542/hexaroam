"""Measure COBS commands with simulated output or a real two-channel PIO fixture."""

import argparse
import math
import struct
import time
import zlib

import serial

from uart_test import encode_wire

FIXTURE_COUNTS = {"sink18": 18, "pio2": 2}
FIXTURE_BATCHES = {"sink18": (1, 6, 18), "pio2": (1, 2)}
MAX_ENCODED_SIZE = 256
MAX_DECODED_SIZE = 254


class BenchmarkError(RuntimeError):
    pass


class NackError(BenchmarkError):
    def __init__(self, command: int, code: int):
        self.command = command
        self.code = code
        super().__init__(f"Command {command:#04x} rejected: error {code}")


def frame(content: bytes) -> bytes:
    return encode_wire(content, "cobs_crc32")


def cobs_decode(encoded: bytes) -> bytes:
    """Decode one response body without its zero delimiters."""
    if not encoded:
        raise BenchmarkError("Empty COBS content")

    decoded = bytearray()
    offset = 0

    while offset < len(encoded):
        code = encoded[offset]
        if code == 0:
            raise BenchmarkError("Zero inside COBS content")

        offset += 1
        end = offset + code - 1
        if end > len(encoded):
            raise BenchmarkError("Truncated COBS block")

        block = encoded[offset:end]
        if 0 in block:
            raise BenchmarkError("Zero inside COBS block")

        decoded.extend(block)
        offset = end

        if code != 255 and offset < len(encoded):
            decoded.append(0)

        if len(decoded) > MAX_DECODED_SIZE:
            raise BenchmarkError("Decoded response too large")

    return bytes(decoded)


def receive_frame(port, deadline: float) -> tuple[bytes, float]:
    started = False
    encoded = bytearray()
    original_timeout = port.timeout

    try:
        while True:
            remaining = deadline - time.perf_counter()
            if remaining <= 0:
                raise TimeoutError(
                    f"Incomplete COBS response: {len(encoded)} encoded bytes"
                )

            # Normally leave the driver's short read timeout unchanged.
            # Only shorten it when the overall deadline is closer.
            if port.timeout > remaining:
                port.timeout = remaining

            chunk = port.read(min(4096, max(1, port.in_waiting)))
            received_at = time.perf_counter()
            if received_at > deadline:
                raise TimeoutError("Response deadline exceeded")

            for index, byte in enumerate(chunk):
                if not started:
                    if byte != 0:
                        raise BenchmarkError("Response lacks start delimiter")
                    started = True
                    continue

                if byte != 0:
                    if len(encoded) == MAX_ENCODED_SIZE:
                        raise BenchmarkError("Encoded response too large")
                    encoded.append(byte)
                    continue

                if not encoded:
                    continue

                if index + 1 != len(chunk) or port.in_waiting:
                    raise BenchmarkError("Unexpected trailing response bytes")

                return cobs_decode(bytes(encoded)), received_at
    finally:
        if port.timeout != original_timeout:
            port.timeout = original_timeout


def exchange(
    port,
    request: bytes,
    command: int,
    result_length: int,
    timeout: float,
) -> tuple[bytes, float]:
    if port.in_waiting:
        raise BenchmarkError("Unexpected bytes before request")

    started = time.perf_counter()
    deadline = started + timeout

    if port.write(request) != len(request):
        raise BenchmarkError("Incomplete request transmission")

    decoded, finished = receive_frame(port, deadline)

    # ACK/NACK, original command and CRC32 need at least six bytes.
    if len(decoded) < 6:
        raise BenchmarkError("Response too short")

    content = decoded[:-4]
    received_crc = struct.unpack("<I", decoded[-4:])[0]
    if received_crc != zlib.crc32(content):
        raise BenchmarkError("Response CRC mismatch")

    if content[1] != command:
        raise BenchmarkError("Unexpected response command")

    if content[0] == 0x81:
        if len(content) != 3:
            raise BenchmarkError("Unexpected NACK length")
        raise NackError(command, content[2])

    if content[0] != 0x80:
        raise BenchmarkError("Unexpected response opcode")

    if len(content) != 2 + result_length:
        raise BenchmarkError("Unexpected ACK length")

    return content[2:], finished - started


def check_status(port, expected: int, timeout: float) -> None:
    payload, _ = exchange(port, frame(b"\x01"), 0x01, 4, timeout)
    actual = struct.unpack("<I", payload)[0]
    if actual != expected:
        raise BenchmarkError(
            f"Status: expected {expected:#010x}, received {actual:#010x}"
        )


def target_request(size: int, angle: int, opposed: bool = False) -> bytes:
    # SET_TARGETS, Servo 0, then signed little-endian centidegrees.
    angles = [angle] * size
    if opposed and size == 2:
        angles[1] = -angle
    content = b"\x03\x00" + struct.pack("<" + "i" * size, *angles)
    return frame(content)


def prepare(port, timeout: float, fixture: str) -> None:
    check_status(port, 1, timeout)
    print("[PASS] Boot status: calibrated and disabled")
    if fixture == "pio2":
        print("Required fixture: uart_commands, unloaded GP0/GP1, real PIO/DMA")
    else:
        print("Required fixture: uart_benchmark, 18 synthetic servos, no GPIO output")

    exchange(port, frame(b"\x02\x01"), 0x02, 0, timeout)
    exchange(port, target_request(FIXTURE_COUNTS[fixture], 0), 0x03, 0, timeout)
    check_status(port, 7, timeout)
    print("[PASS] Enabled and complete initial target set accepted")


def run_batch(
    port,
    size: int,
    duration: float,
    timeout: float,
    fixture: str,
    rate: float | None,
) -> bool:
    physical_output = fixture == "pio2"
    amplitude = 4_500 if physical_output else 1_000
    requests = [
        target_request(size, angle, opposed=physical_output)
        for angle in (-amplitude, amplitude)
    ]
    count = 0
    attempts = 0
    busy = 0
    other_nacks = 0
    skipped = 0
    next_slot = 0
    last_send = None
    interval = None if rate is None else 1.0 / rate
    total_latency = 0.0
    maximum_latency = 0.0
    timeouts = 0
    invalid = 0
    transport_errors = 0
    failure = None
    started = time.perf_counter()
    end = started + duration

    try:
        while True:
            now = time.perf_counter()
            if now >= end:
                break

            if interval is not None and last_send is not None:
                wait = last_send + interval - now
                if wait > 0:
                    # An intentional rate limit, never used in unlimited mode.
                    # Do not sleep past the end of the measurement window.
                    time.sleep(min(wait, end - now))
                    continue

            if interval is None:
                sequence = attempts
            else:
                # Choose the current wall-clock pattern, not an old queued
                # pose. Missing whole schedule slots are skipped, not replayed.
                sequence = max(next_slot, int((now - started) / interval))
                skipped += sequence - next_slot
                next_slot = sequence + 1

            last_send = now
            attempts += 1
            try:
                _, latency = exchange(
                    port, requests[sequence % 2], 0x03, 0, timeout
                )
            except NackError as error:
                if physical_output and error.command == 0x03 and error.code == 0x05:
                    # This request was rejected. Advance to a fresh pattern
                    # on the next iteration; do not retry this stale request.
                    busy += 1
                    continue
                raise

            count += 1
            total_latency += latency
            maximum_latency = max(maximum_latency, latency)
    except NackError as error:
        other_nacks = 1
        failure = str(error)
    except TimeoutError as error:
        timeouts = 1
        failure = str(error)
    except BenchmarkError as error:
        invalid = 1
        failure = str(error)
    except (serial.SerialException, OSError) as error:
        transport_errors = 1
        failure = str(error)

    elapsed = time.perf_counter() - started

    # A run that only received Busy is not a successful update test.
    if failure is None and count == 0:
        failure = "No target request was accepted"

    # Status is checked outside the measured interval. It does not
    # read back target angles or verify physical pulse generation.
    if failure is None:
        try:
            check_status(port, 7, timeout)
        except (BenchmarkError, TimeoutError, serial.SerialException, OSError) as error:
            failure = f"Final status check: {error}"

    mean_ms = total_latency / count * 1_000 if count else 0.0
    requests_per_second = count / elapsed if elapsed else 0.0
    busy_percent = busy / attempts * 100 if attempts else 0.0
    rate_label = "unlimited" if rate is None else f"at most {rate:g}/s"

    print(f"\nFixture: {fixture} | Baud: {port.baudrate} | Batch: {size}")
    print(f"Duration: {elapsed:.2f} s | Requested rate: {rate_label}")
    print(
        f"Requests: {attempts} attempted, {count} ACK, "
        f"{busy + other_nacks} NACK ({busy} Busy, {other_nacks} other)"
    )
    print(f"Busy: {busy_percent:.1f}% | Skipped schedule slots: {skipped}")
    print(
        f"Throughput: {requests_per_second:.1f} accepted requests/s, "
        f"{requests_per_second * size:.1f} targets/s"
    )
    print(
        f"ACK round trip: mean {mean_ms:.3f} ms, "
        f"max {maximum_latency * 1_000:.3f} ms"
    )
    print(
        f"Errors during measurement: {timeouts} timeout, "
        f"{invalid} invalid response, {transport_errors} transport"
    )

    if failure:
        print(f"[FAIL] {failure}")
        return False

    state = "running" if physical_output else "logically running"
    print(f"[PASS] Final status: calibrated, enabled, {state}")
    return True


def positive_seconds(value: str) -> float:
    result = float(value)
    if not math.isfinite(result) or result <= 0:
        raise argparse.ArgumentTypeError("Must be a finite positive number")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", required=True)
    parser.add_argument(
        "--fixture", choices=tuple(FIXTURE_COUNTS), default="sink18",
        help="sink18: uart_benchmark; pio2: uart_commands with unloaded GP0/GP1",
    )
    parser.add_argument(
        "--baud", type=int, choices=(115200, 128000, 256000), default=115200
    )
    parser.add_argument("--duration", type=positive_seconds, default=30.0)
    parser.add_argument("--timeout", type=positive_seconds, default=1.0)
    parser.add_argument("--batch-size", type=int, choices=(1, 2, 6, 18))
    parser.add_argument(
        "--rate", type=positive_seconds,
        help="Maximum requests/s for pio2; omit for an unlimited overload run",
    )
    args = parser.parse_args()

    if args.batch_size is not None and args.batch_size not in FIXTURE_BATCHES[args.fixture]:
        parser.error("Batch size is not supported by the selected fixture")
    if args.fixture == "pio2" and args.baud != 115200:
        parser.error("The uart_commands firmware uses a fixed 115200 baud")
    if args.rate is not None and args.fixture != "pio2":
        parser.error("--rate is reserved for the pio2 sustained-output test")

    try:
        with serial.Serial(
            port=args.port,
            baudrate=args.baud,
            bytesize=serial.EIGHTBITS,
            parity=serial.PARITY_NONE,
            stopbits=serial.STOPBITS_ONE,
            timeout=min(0.01, args.timeout),
            write_timeout=args.timeout,
            xonxoff=False,
            rtscts=False,
            dsrdtr=False,
        ) as port:
            prepare(port, args.timeout, args.fixture)

            sizes = (
                (args.batch_size,)
                if args.batch_size is not None
                else FIXTURE_BATCHES[args.fixture]
            )
            for size in sizes:
                if not run_batch(
                    port, size, args.duration, args.timeout, args.fixture, args.rate
                ):
                    print("Reset the Pico before another attempt.")
                    return 1

            exchange(port, frame(b"\x02\x00"), 0x02, 0, args.timeout)
            check_status(port, 1, args.timeout)
            print("\n[PASS] Output permission cleared")

    except KeyboardInterrupt:
        print("\nInterrupted. Reset the Pico before another attempt.")
        return 130
    except (BenchmarkError, TimeoutError, serial.SerialException, OSError) as error:
        print(f"[FAIL] {error}")
        print("Reset the Pico before another attempt.")
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
