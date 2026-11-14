"""Shared CRC32/COBS framing and synchronous command exchange."""

import struct
import time
import zlib

MAX_ENCODED_SIZE = 256
MAX_DECODED_SIZE = 254


class ProtocolError(RuntimeError):
    pass


class NackError(ProtocolError):
    def __init__(self, command: int, code: int):
        self.command = command
        self.code = code
        super().__init__(f"Command {command:#04x} rejected: error {code}")


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


def frame(content: bytes) -> bytes:
    return encode_wire(content, "cobs_crc32")


def cobs_decode(encoded: bytes) -> bytes:
    """Decode one response body without its zero delimiters."""
    if not encoded:
        raise ProtocolError("Empty COBS content")

    decoded = bytearray()
    offset = 0

    while offset < len(encoded):
        code = encoded[offset]
        if code == 0:
            raise ProtocolError("Zero inside COBS content")

        offset += 1
        end = offset + code - 1
        if end > len(encoded):
            raise ProtocolError("Truncated COBS block")

        block = encoded[offset:end]
        if 0 in block:
            raise ProtocolError("Zero inside COBS block")

        decoded.extend(block)
        offset = end

        if code != 255 and offset < len(encoded):
            decoded.append(0)

        if len(decoded) > MAX_DECODED_SIZE:
            raise ProtocolError("Decoded response too large")

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
                        raise ProtocolError("Response lacks start delimiter")
                    started = True
                    continue

                if byte != 0:
                    if len(encoded) == MAX_ENCODED_SIZE:
                        raise ProtocolError("Encoded response too large")
                    encoded.append(byte)
                    continue

                if not encoded:
                    continue

                if index + 1 != len(chunk) or port.in_waiting:
                    raise ProtocolError("Unexpected trailing response bytes")

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
        raise ProtocolError("Unexpected bytes before request")

    started = time.perf_counter()
    deadline = started + timeout

    if port.write(request) != len(request):
        raise ProtocolError("Incomplete request transmission")

    decoded, finished = receive_frame(port, deadline)

    # ACK/NACK, original command and CRC32 need at least six bytes.
    if len(decoded) < 6:
        raise ProtocolError("Response too short")

    content = decoded[:-4]
    received_crc = struct.unpack("<I", decoded[-4:])[0]
    if received_crc != zlib.crc32(content):
        raise ProtocolError("Response CRC mismatch")

    if content[1] != command:
        raise ProtocolError("Unexpected response command")

    if content[0] == 0x81:
        if len(content) != 3:
            raise ProtocolError("Unexpected NACK length")
        raise NackError(command, content[2])

    if content[0] != 0x80:
        raise ProtocolError("Unexpected response opcode")

    if len(content) != 2 + result_length:
        raise ProtocolError("Unexpected ACK length")

    return content[2:], finished - started
