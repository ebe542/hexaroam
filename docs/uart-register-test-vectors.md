# UART register test vectors

Reference data for [uart_registers.rs](../firmware/pico/hardware_tests/uart_registers.rs). These vectors describe the current manual test fixture, not a production configuration or a released protocol-version convention.

## Fixture and execution

The fixture uses NoServoOutput, protocol version 1, no valid calibration and zero configured servos. UART0 uses GP16/TX and GP17/RX at 115200 baud, 8N1, without hardware flow control. Diagnostics are separate on UART1/GP20. Remove the GP16-GP17 loopback jumper before connecting the PC adapter. See the [hardware procedure](../firmware/pico/hardware_tests/README.md#uart-register-requestresponse-test) for wiring and firmware build/upload.

In HTerm select HEX input and HEX receive display, with no automatic CR/LF. Each space-separated pair below is one binary byte, not ASCII text. After a READ, wait for its complete response before sending another request. Operations without replies follow the observation intervals defined in the extended suite below. There is no unsolicited startup message on the protocol UART.

## Wire layout

| Field | Size | Meaning |
| --- | ---: | --- |
| Opcode | 1 byte | 01 = READ, 02 = WRITE, 03 = SET, 04 = CLR |
| Register index | 1 byte | 00 = version, 01 = status, 02 = LastError, 03 = Control, 04 = servo count; 20 starts targets |
| Length | 2 bytes, little endian | Requested response payload size |
| Payload | WRITE/SET/CLR requests and READ responses | Scalars use their register width; READ requests carry none |
| CRC | 4 bytes, little endian | CRC-32/ISO-HDLC over header and any payload |

A READ request has no payload and is always eight bytes. Its Length still specifies the expected response payload. Version replies contain ten bytes; status/count replies contain nine. Spaces and field separators shown here are not transmitted.

## Verified vectors

| Case | Expected value | Request (TX) | Response (RX) |
| --- | --- | --- | --- |
| ProtocolVersion | 1, payload 01 00 | `01 00 02 00 FB DA CE AB` | `01 00 02 00 01 00 CC 8B 8C C9` |
| Status | 0, payload 00 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| ServoCount | 0, payload 00 | `01 04 01 00 E4 21 EA 87` | `01 04 01 00 00 CD 23 E2 75` |

Version request split into fields:

```text
01 | 00 | 02 00 | FB DA CE AB
READ index length CRC
```

Version response:

```text
01 | 00 | 02 00 | 01 00 | CC 8B 8C C9
READ index length payload CRC
```

Status zero means no calibration-valid, output-active or fault bits are set in this fixture. ServoCount zero is intentional because this test owns no servo outputs. It is not the capacity planned for the final controller.

## Reproduce the data independently

CRC-32/ISO-HDLC uses polynomial 0x04C11DB7 (reflected form 0xEDB88320), initial register 0xFFFFFFFF, reflected input/output and final XOR 0xFFFFFFFF. Its standard check value for ASCII 123456789 is 0xCBF43926. CRC covers the complete header and response payload when present, excluding the CRC field itself.

Python's standard-library zlib.crc32 with its default initial argument calculates this CRC directly. Do not substitute the CRC register initialization value as zlib's initial argument.

From Git Bash or a Linux shell, run the following to print all requests and expected responses. It uses no external packages, writes no files and sends no serial data:

```bash
python - <<'PY'
import struct
import zlib


def with_crc(body):
    return body + struct.pack("<I", zlib.crc32(body))


fixtures = [
    ("ProtocolVersion", 0x00, struct.pack("<H", 1)),
    ("Status", 0x01, bytes([0])),
    ("ServoCount", 0x04, bytes([0])),
]

for name, index, payload in fixtures:
    header = struct.pack("<BBH", 0x01, index, len(payload))
    request = with_crc(header)
    response = with_crc(header + payload)
    print(name)
    print("TX:", request.hex(" ").upper())
    print("RX:", response.hex(" ").upper())
PY
```

## Verification record and limits

On 2026-11-02 the owner confirmed that all three HTerm responses matched these reference bytes. Full verification reported 451 tests and all 17 checks passing. These three cases verify valid READ exchanges and their exact expected CRC bytes. They do not cover error replies, corrupted frames, incomplete-frame timeout recovery, UART error injection, calibration transfer or servo motion. Add future cases with their fixture, complete wire bytes and expected outcome, and distinguish generated expectations from hardware-confirmed results.

## Extended register suite

The [machine-readable fixture](../firmware/pico/hardware_tests/suites/uart_registers.json) contains 21 scenarios with complete transmit bytes, expected reply bytes or an explicit no-reply expectation, timing and initial-state setup. On 2026-11-02 the owner reported a successful automated run of all 21 scenarios against real Pico hardware using `tools/uart_test.py`: **21 passed, 0 failed, 0 not completed; Overall: PASSED**. Scenarios 1 through 12 had also been confirmed manually. This suite covers the documented register exchanges, error handling, corrupted frames and incomplete-frame timeout recovery with the no-servo fixture; calibration transfer, servo motion and UART hardware error injection remain outside this verification.

The same uart_registers firmware supports these cases. Before each scenario, clear LastError and read it back as shown below. Control and Status must remain zero throughout this fixture. Send every sequence as binary HEX without CR/LF. For each modifying, corrupt or partial transmission expecting no reply, observe the link for at least 200 ms before the next step; this also lets the test firmware's provisional 100 ms idle timeout retire incomplete resynchronization candidates. For READs, await and compare the complete expected reply, allowing up to one second for this manual procedure.

Important distinctions:

- CRC-valid READ failures produce a READ response at LastError index 02 with a four-byte error value.
- Valid WRITE/SET/CLR requests produce no direct reply, including semantic rejections; inspect LastError explicitly.
- Non-READ headers with unknown indices, read-only destinations or unsupported access are discarded early by the parser. They do not reach the register service and do not change LastError.
- With ServoCount=0, target access returns OutOfBounds before calibration is checked. Successful target writes/reads and output activation require a different fixture.
- An error register value does not itself imply that the technical Fault status bit is set.

### Baseline before every scenario

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Clear LastError | `02 02 04 00 00 00 00 00 2B 8B 09 2B` | No reply; observe 200 ms |
| 2 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

### Scenario index

| Number | ID | Scenario |
| ---: | --- | --- |
| 1 | read_version | Read ProtocolVersion |
| 2 | read_status | Read Status |
| 3 | read_last_error | Read LastError |
| 4 | read_control | Read Control |
| 5 | read_servo_count | Read ServoCount |
| 6 | enable_write | WRITE enable without calibration |
| 7 | enable_set | SET enable without calibration |
| 8 | disable_preserves_error | Disable operations preserve previous LastError |
| 9 | reserved_write | WRITE reserved Control bits |
| 10 | reserved_set | SET reserved Control bits |
| 11 | reserved_clr | CLR reserved Control bits |
| 12 | last_error_write_and_clear | Reject nonzero LastError and clear explicitly |
| 13 | readonly_version | WRITE to read-only version |
| 14 | readonly_status | WRITE to read-only status |
| 15 | readonly_servo_count | WRITE to read-only servo_count |
| 16 | unknown_read | READ unknown register |
| 17 | invalid_read_lengths | Reject invalid READ lengths for every scalar |
| 18 | targets_without_servos | Reject target access with ServoCount zero |
| 19 | discarded_write_headers | Discard unknown or unsupported non-READ headers |
| 20 | bad_crc | Ignore a damaged CRC and accept the next request |
| 21 | incomplete_timeout | Discard a timed-out header before its old CRC arrives |

### 1. Read ProtocolVersion

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Read ProtocolVersion = 1 | `01 00 02 00 FB DA CE AB` | `01 00 02 00 01 00 CC 8B 8C C9` |

### 2. Read Status

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Read Status = 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |

### 3. Read LastError

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

### 4. Read Control

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Read Control = 0 | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |

### 5. Read ServoCount

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Read ServoCount = 0 | `01 04 01 00 E4 21 EA 87` | `01 04 01 00 00 CD 23 E2 75` |

### 6. WRITE enable without calibration

NotConfigured = 6. No physical output starts.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | WRITE Control enable | `02 03 01 00 01 32 51 92 D8` | No reply; observe 200 ms |
| 2 | Control remains 0 | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |
| 3 | Status remains 0; no technical Fault | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 4 | Read LastError = 6 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 06 00 00 00 14 D3 ED 80` |

### 7. SET enable without calibration

NotConfigured = 6. No physical output starts.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | SET Control enable | `03 03 01 00 01 82 78 F2 E5` | No reply; observe 200 ms |
| 2 | Control remains 0 | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |
| 3 | Status remains 0; no technical Fault | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 4 | Read LastError = 6 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 06 00 00 00 14 D3 ED 80` |

### 8. Disable operations preserve previous LastError

Successful WRITE/CLR and READs do not clear LastError. Output was inactive throughout.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | SET enable to establish NotConfigured | `03 03 01 00 01 82 78 F2 E5` | No reply; observe 200 ms |
| 2 | Read LastError = 6 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 06 00 00 00 14 D3 ED 80` |
| 3 | WRITE Control = 0 | `02 03 01 00 00 A4 61 95 AF` | No reply; observe 200 ms |
| 4 | CLR enable | `04 03 01 00 01 92 A4 D2 57` | No reply; observe 200 ms |
| 5 | Control remains 0 | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |
| 6 | Status remains 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 7 | Read LastError = 6 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 06 00 00 00 14 D3 ED 80` |

### 9. WRITE reserved Control bits

InvalidValue = 5. WRITE uses mask 03 to check reserved-bit validation before missing calibration.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | WRITE reserved Control mask | `02 03 01 00 03 1E 30 9C 36` | No reply; observe 200 ms |
| 2 | Control remains 0 | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |
| 3 | Status remains 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 4 | Read LastError = 5 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 05 00 00 00 FA 7C 58 92` |

### 10. SET reserved Control bits

InvalidValue = 5. WRITE uses mask 03 to check reserved-bit validation before missing calibration.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | SET reserved Control mask | `03 03 01 00 02 38 29 FB 7C` | No reply; observe 200 ms |
| 2 | Control remains 0 | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |
| 3 | Status remains 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 4 | Read LastError = 5 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 05 00 00 00 FA 7C 58 92` |

### 11. CLR reserved Control bits

InvalidValue = 5. WRITE uses mask 03 to check reserved-bit validation before missing calibration.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | CLR reserved Control mask | `04 03 01 00 02 28 F5 DB CE` | No reply; observe 200 ms |
| 2 | Control remains 0 | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |
| 3 | Status remains 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 4 | Read LastError = 5 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 05 00 00 00 FA 7C 58 92` |

### 12. Reject nonzero LastError and clear explicitly

Only a four-byte WRITE of zero clears LastError. Reading it does not clear it.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | WRITE LastError = 1 | `02 02 04 00 01 00 00 00 4E EC B5 93` | No reply; observe 200 ms |
| 2 | Read LastError = 5 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 05 00 00 00 FA 7C 58 92` |
| 3 | Successful Control read | `01 03 01 00 61 37 A5 82` | `01 03 01 00 00 74 1B 35 E8` |
| 4 | Read LastError = 5 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 05 00 00 00 FA 7C 58 92` |
| 5 | WRITE LastError = 0 | `02 02 04 00 00 00 00 00 2B 8B 09 2B` | No reply; observe 200 ms |
| 6 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

### 13. WRITE to read-only version

The parser rejects this non-READ header before register dispatch. LastError remains unchanged, not UnsupportedAccess.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Attempt WRITE to read-only register | `02 00 02 00 EF BE 8B B9 3C A3` | No reply; observe 200 ms |
| 2 | Read original value | `01 00 02 00 FB DA CE AB` | `01 00 02 00 01 00 CC 8B 8C C9` |
| 3 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

### 14. WRITE to read-only status

The parser rejects this non-READ header before register dispatch. LastError remains unchanged, not UnsupportedAccess.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Attempt WRITE to read-only register | `02 01 01 00 FF A2 46 9E 28` | No reply; observe 200 ms |
| 2 | Read original value | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 3 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

### 15. WRITE to read-only servo_count

The parser rejects this non-READ header before register dispatch. LastError remains unchanged, not UnsupportedAccess.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Attempt WRITE to read-only register | `02 04 01 00 FF 90 B6 40 1F` | No reply; observe 200 ms |
| 2 | Read original value | `01 04 01 00 E4 21 EA 87` | `01 04 01 00 00 CD 23 E2 75` |
| 3 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

### 16. READ unknown register

UnknownIndex = 1. The direct READ error response uses LastError index 02 and length 4.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | READ unknown index FF | `01 FF 01 00 D5 BD 7B 3E` | `01 02 04 00 01 00 00 00 AD EB 3A 1D` |
| 2 | Read LastError = 1 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 01 00 00 00 AD EB 3A 1D` |
| 3 | Status remains 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |

### 17. Reject invalid READ lengths for every scalar

InvalidLength = 3; OutOfBounds = 4. A CRC-valid rejected READ produces a canonical error response.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Version: one byte instead of two | `01 00 01 00 38 89 E3 80` | `01 02 04 00 03 00 00 00 26 23 33 B7` |
| 2 | Status: two bytes cross scalar boundary | `01 01 02 00 CC B0 0C AA` | `01 02 04 00 04 00 00 00 9F 1B E4 2A` |
| 3 | LastError: one byte instead of four | `01 02 01 00 56 5D 67 83` | `01 02 04 00 03 00 00 00 26 23 33 B7` |
| 4 | Control: two bytes cross scalar boundary | `01 03 02 00 A2 64 88 A9` | `01 02 04 00 04 00 00 00 9F 1B E4 2A` |
| 5 | ServoCount: zero-length READ | `01 04 00 00 A5 10 F1 9E` | `01 02 04 00 03 00 00 00 26 23 33 B7` |
| 6 | Read LastError = 3 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 03 00 00 00 26 23 33 B7` |

### 18. Reject target access with ServoCount zero

OutOfBounds = 4 is checked before calibration validity because ServoCount is zero. This does not test successful angle access.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | READ target 0 while no servo is configured | `01 20 04 00 9D 3B D9 C5` | `01 02 04 00 04 00 00 00 9F 1B E4 2A` |
| 2 | Clear LastError | `02 02 04 00 00 00 00 00 2B 8B 09 2B` | No reply; observe 200 ms |
| 3 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |
| 4 | WRITE target 0 = 0 centidegrees | `02 20 04 00 00 00 00 00 9E A3 2B 73` | No reply; observe 200 ms |
| 5 | Read LastError = 4 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 04 00 00 00 9F 1B E4 2A` |
| 6 | Status remains 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |

### 19. Discard unknown or unsupported non-READ headers

These headers are discarded by the parser. The register service is not called, so LastError stays zero.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | WRITE unknown index FF | `02 FF 01 00 55 C0 6D 24 78` | No reply; observe 200 ms |
| 2 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |
| 3 | SET LastError is unsupported | `03 02 04 00 FF FF FF FF 56 AB 18 39` | No reply; observe 200 ms |
| 4 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |
| 5 | CLR LastError is unsupported | `04 02 04 00 FF FF FF FF 4F A2 DD 33` | No reply; observe 200 ms |
| 6 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |
| 7 | Status remains 0 | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |

### 20. Ignore a damaged CRC and accept the next request

No reply is expected for the damaged frame. The parser does not update the register LastError.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | READ Status with corrupted final CRC byte | `01 01 01 00 0F E3 21 01` | No reply; observe 200 ms |
| 2 | Valid READ Status after the observation interval | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 3 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

### 21. Discard a timed-out header before its old CRC arrives

Wait the full 200 ms after the header and after the old CRC. The old CRC alone must not complete the expired READ or produce a response.

| Step | Action | TX bytes | Expected RX |
| ---: | --- | --- | --- |
| 1 | Send only the four-byte READ Status header | `01 01 01 00` | No reply; observe 200 ms |
| 2 | Send only its old CRC after the timeout | `0F E3 21 81` | No reply; observe 200 ms |
| 3 | Send a new complete READ Status | `01 01 01 00 0F E3 21 81` | `01 01 01 00 00 FF D3 3C 42` |
| 4 | Read LastError = 0 | `01 02 04 00 13 A9 10 FE` | `01 02 04 00 00 00 00 00 C8 8C 86 A5` |

## Reproduce the extended suite

The JSON records request bodies separately from wire bytes. The following standard-library command regenerates the complete suite, including the deliberately bad CRC and raw partial transmissions. It reads the fixture and prints expectations only; it performs no serial I/O and does not execute hardware tests. Run from the repository root in Git Bash or a Linux shell.

```bash
python - <<'PY'
import json
from pathlib import Path
import struct
import zlib


def with_crc(body):
    return body + struct.pack("<I", zlib.crc32(body))


def show(steps):
    for number, step in enumerate(steps, 1):
        body = bytes.fromhex(step["request_body_hex"])
        tx = body if step["encoding"] == "raw" else with_crc(body)
        if step["encoding"] == "bad_crc":
            tx = tx[:-1] + bytes([tx[-1] ^ 0x80])
        reply = step["response_body_hex"]
        rx = None if reply is None else with_crc(bytes.fromhex(reply))
        print(number, step["action"])
        print(" TX:", tx.hex(" ").upper())
        print(" RX:", "no reply" if rx is None else rx.hex(" ").upper())


fixture = json.loads(
    Path("firmware/pico/hardware_tests/suites/uart_registers.json").read_text(encoding="utf-8")
)
print("Baseline before every case")
show(fixture["before_each_case"])
for case in fixture["cases"]:
    print("\n" + case["id"] + ": " + case["title"])
    show(case["steps"])
PY
```

The suite covers all five currently implemented scalar registers and rejection of the target region under this zero-servo fixture. Calibration block access is not implemented. Successful calibrated targets, actual output start/stop, physical UART error injection and sustained operation remain separate tests. No expanded hardware result or changed unit-test count is claimed by generating these data.

### Manual progress

On 2026-11-02 the owner confirmed successful manual execution through scenario 12 (LastError nonzero rejection and explicit clearing). This covers the scalar reads, missing-calibration enable rejection, successful disable operations preserving LastError and reserved Control bits. Scenarios 13..21 are not marked successful. A serial runner using this JSON fixture is the next step; no automated result is implied by these manual observations.
