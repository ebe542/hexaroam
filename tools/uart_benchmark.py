"""Measure COBS command throughput against the 18-servo benchmark firmware."""

import argparse
import math
import struct
import time
import zlib

import serial

from uart_test import encode_wire

SERVO_COUNT = 18
MAX_ENCODED_SIZE = 256
MAX_DECODED_SIZE = 254


class BenchmarkError(RuntimeError):
    pass


class NackError(BenchmarkError):
    pass


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
        raise NackError(
            f"Command {command:#04x} rejected: error {content[2]}"
        )

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


def target_request(size: int, angle: int) -> bytes:
    # SET_TARGETS, Servo 0, then signed little-endian centidegrees.
    content = b"\x03\x00" + struct.pack("<" + "i" * size, *([angle] * size))
    return frame(content)


def prepare(port, timeout: float) -> None:
    check_status(port, 1, timeout)
    print("[PASS] Boot status: calibrated and disabled")
    print("Required fixture: uart_benchmark, 18 synthetic servos, no GPIO output")

    exchange(port, frame(b"\x02\x01"), 0x02, 0, timeout)
    exchange(port, target_request(SERVO_COUNT, 0), 0x03, 0, timeout)
    check_status(port, 7, timeout)
    print("[PASS] Enabled and complete initial target set accepted")


def run_batch(port, size: int, duration: float, timeout: float) -> bool:
    requests = [target_request(size, angle) for angle in (-1_000, 1_000)]
    count = 0
    total_latency = 0.0
    maximum_latency = 0.0
    nacks = 0
    timeouts = 0
    invalid = 0
    transport_errors = 0
    failure = None
    started = time.perf_counter()

    try:
        while time.perf_counter() - started < duration:
            _, latency = exchange(
                port, requests[count % 2], 0x03, 0, timeout
            )
            count += 1
            total_latency += latency
            maximum_latency = max(maximum_latency, latency)
    except NackError as error:
        nacks = 1
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

    # Status is checked outside the measured interval. It does not
    # read back target angles or verify physical pulse generation.
    if failure is None:
        try:
            check_status(port, 7, timeout)
        except (BenchmarkError, TimeoutError, serial.SerialException, OSError) as error:
            failure = f"Final status check: {error}"

    mean_ms = total_latency / count * 1_000 if count else 0.0
    requests_per_second = count / elapsed if elapsed else 0.0

    print(f"\nBaud: {port.baudrate} | Batch: {size} | Duration: {elapsed:.2f} s")
    print(f"Requests: {count} ACK, {nacks} NACK")
    print(
        f"Throughput: {requests_per_second:.1f} requests/s, "
        f"{requests_per_second * size:.1f} targets/s"
    )
    print(
        f"Round trip: mean {mean_ms:.3f} ms, "
        f"max {maximum_latency * 1_000:.3f} ms"
    )
    print(
        f"Errors during measurement: {timeouts} timeout, "
        f"{invalid} invalid response, {transport_errors} transport"
    )

    if failure:
        print(f"[FAIL] {failure}")
        return False

    print("[PASS] Final status: calibrated, enabled, logically running")
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
        "--baud", type=int, choices=(115200, 128000, 256000), default=115200
    )
    parser.add_argument("--duration", type=positive_seconds, default=30.0)
    parser.add_argument("--timeout", type=positive_seconds, default=1.0)
    parser.add_argument("--batch-size", type=int, choices=(1, 6, 18))
    args = parser.parse_args()

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
            prepare(port, args.timeout)

            sizes = (args.batch_size,) if args.batch_size else (1, 6, 18)
            for size in sizes:
                if not run_batch(port, size, args.duration, args.timeout):
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
