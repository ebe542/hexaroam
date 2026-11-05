"""Measure register-protocol throughput against real Pico hardware."""

import argparse
import math
import struct
import time
import zlib

import serial


class BenchmarkError(RuntimeError):
    pass


class NackError(BenchmarkError):
    pass


def frame(opcode: int, index: int, payload: bytes) -> bytes:
    body = struct.pack("<BBH", opcode, index, len(payload)) + payload
    return body + struct.pack("<I", zlib.crc32(body))


def read_request(index: int, length: int) -> bytes:
    body = struct.pack("<BBH", 1, index, length)
    return body + struct.pack("<I", zlib.crc32(body))


def read_exact(port, length: int, deadline: float) -> bytes:
    received = bytearray()

    while len(received) < length:
        remaining = deadline - time.perf_counter()
        if remaining <= 0:
            raise TimeoutError(f"Incomplete response: {len(received)}/{length} bytes")

        port.timeout = remaining
        part = port.read(length - len(received))
        received.extend(part)

    return bytes(received)


def exchange(
    port,
    request: bytes,
    index: int,
    expected_length: int,
    timeout: float,
) -> tuple[bytes, float]:
    if port.in_waiting:
        raise BenchmarkError("Unexpected bytes before request")

    started = time.perf_counter()
    deadline = started + timeout

    if port.write(request) != len(request):
        raise BenchmarkError("Incomplete request transmission")

    header = read_exact(port, 4, deadline)
    opcode, response_index, length = struct.unpack("<BBH", header)

    if response_index != index:
        raise BenchmarkError("Unexpected response register")

    if opcode == 0x80:
        if length != expected_length:
            raise BenchmarkError(f"Unexpected ACK length: {length}")
    elif opcode == 0x81:
        if length != 1:
            raise BenchmarkError(f"Unexpected NACK length: {length}")
    else:
        raise BenchmarkError(f"Unexpected response opcode: {opcode:#04x}")

    tail = read_exact(port, length + 4, deadline)
    finished = time.perf_counter()
    payload = tail[:-4]
    received_crc = struct.unpack("<I", tail[-4:])[0]

    if received_crc != zlib.crc32(header + payload):
        raise BenchmarkError("Response CRC mismatch")

    if port.in_waiting:
        raise BenchmarkError("Unexpected trailing bytes")

    if opcode == 0x81:
        raise NackError(f"Controller rejected request: error {payload[0]}")

    return payload, finished - started


def read_register(port, index: int, length: int, timeout: float) -> bytes:
    payload, _ = exchange(port, read_request(index, length), index, length, timeout)
    return payload


def check_fixture(port, timeout: float) -> None:
    expected = (
        (0x00, struct.pack("<H", 2), "ProtocolVersion"),
        (0x01, struct.pack("<I", 1), "Status"),
        (0x04, bytes([18]), "ServoCount"),
    )

    for index, value, name in expected:
        actual = read_register(port, index, len(value), timeout)
        if actual != value:
            raise BenchmarkError(
                f"{name}: expected {value.hex(' ')}, received {actual.hex(' ')}"
            )

    print("[PASS] Fixture check")


def run_batch(port, size: int, duration: float, timeout: float) -> bool:
    # Alternate between -10 and +10 degrees, within the synthetic limits.
    payloads = [
        struct.pack("<" + "i" * size, *([value] * size)) for value in (-1_000, 1_000)
    ]
    requests = [frame(2, 0x20, payload) for payload in payloads]

    count = 0
    total_latency = 0.0
    maximum_latency = 0.0
    nacks = 0
    timeouts = 0
    invalid = 0
    failure = None

    started = time.perf_counter()

    try:
        while time.perf_counter() - started < duration:
            _, latency = exchange(port, requests[count % 2], 0x20, 0, timeout)
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

    elapsed = time.perf_counter() - started

    # Readback is outside the timed measurement.
    if failure is None and count:
        actual = read_register(port, 0x20, size * 4, timeout)
        if actual != payloads[(count - 1) % 2]:
            invalid += 1
            failure = "Final target readback differs"

    mean_ms = total_latency / count * 1_000 if count else 0.0

    print(f"\nBaud: {port.baudrate} | Batch: {size} | Duration: {elapsed:.2f} s")
    print(f"Requests: {count} ACK, {nacks} NACK")
    print(
        f"Throughput: {count / elapsed:.1f} requests/s, "
        f"{count * size / elapsed:.1f} targets/s"
    )
    print(f"Round trip: mean {mean_ms:.3f} ms, max {maximum_latency * 1_000:.3f} ms")
    print(f"Errors: {timeouts} timeout, {invalid} invalid response")

    if failure:
        print(f"[FAIL] {failure}")
        return False

    print("[PASS] Final target readback")
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
            timeout=args.timeout,
            write_timeout=args.timeout,
            xonxoff=False,
            rtscts=False,
            dsrdtr=False,
        ) as port:
            check_fixture(port, args.timeout)

            sizes = (args.batch_size,) if args.batch_size else (1, 6, 18)
            for size in sizes:
                if not run_batch(port, size, args.duration, args.timeout):
                    return 1

    except (BenchmarkError, TimeoutError, serial.SerialException) as error:
        print(f"[FAIL] {error}")
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
