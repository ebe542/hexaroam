# UART register test vectors

These vectors target protocol version **2** in [uart_registers.rs](../firmware/pico/hardware_tests/uart_registers.rs). The [JSON suite](../firmware/pico/hardware_tests/suites/uart_registers.json) is the machine-readable source of the tables below.

## Fixture and execution

The fixture uses NoServoOutput: no valid calibration, Status=0, Control=0 and ServoCount=0. Status transfers four little-endian bytes; Control still transfers one byte. Busy and the action-result field remain unimplemented, as do Restart and calibration transfer.

Protocol UART0 uses GP16/TX and GP17/RX at 115200 baud, 8N1, no flow control. Use a 3.3-V TTL adapter and common GND; remove the GP16-GP17 loopback jumper. Debug logging is separate on UART1/GP20. Close HTerm before running the Python script.

Build and flash the updated `uart_registers.uf2` before running this version-2 suite. From the repository root, with the hardware dependencies installed:

```bash
python tools/dev.py hardware-build
python tools/uart_test.py --port COM5 --suite uart_registers
```

Replace COM5 with your device, for example /dev/ttyUSB0 on Ubuntu. See [tool setup and invocation](../tools/README.md#uart-hardware-suites) and the [hardware procedure](../firmware/pico/hardware_tests/README.md#uart-register-requestresponse-test). Use `--case read_status` to select one scenario.

## Wire layout and response rules

`Opcode:u8 | Register:u8 | Length:u16 LE | Payload | CRC32 LE`

- Requests: READ=01, WRITE=02, SET=03, CLR=04. READ has no request payload; its Length specifies the requested data size.
- ACK=80: same register index, requested data for READ, empty payload for successful WRITE/SET/CLR.
- NACK=81: same register index, Length=1, one error byte.
- Unknown register indices and implausible lengths are discarded without a reply. Plausible but prohibited accesses reach the service and return NACK after CRC validation.
- Damaged CRC and incomplete frames produce no reply. The receive-idle timeout discards incomplete candidates; this does not guarantee immediate recovery from all possible corrupt byte sequences.
- Register 02 is unassigned. LastError was removed; immediate errors are returned by NACK.
- The static target region 20..31 is plausible, but ServoCount=0 makes accesses return OutOfBounds (04). Missing calibration rejects output activation with NotConfigured (06). Reserved Control bits return InvalidValue (05); writes to read-only registers return UnsupportedAccess (02).

The master waits for each response before sending another request. Each expected reply is observed for 1000 ms, including detection of extra bytes. No-reply steps observe 200 ms, exceeding the fixture's 100-ms receive-idle timeout. The runner stops at the first failure. It performs read-only preflight checks of version, status and servo count; each scenario starts with Control and Status reads. No clearing of an error register is needed.

For manual HTerm use HEX input/output with no appended CR/LF. All table bytes are binary, not ASCII text.

## Verification record and limits

On 2026-11-03 the owner reported the current four-byte Status suite passing on hardware: **21 passed, 0 failed, 0 not completed**. All 78 register host tests also passed. Full local verification passed 465 tests and all 17 checks; CI for this width change has not yet been reported. Earlier full-verification totals below apply to the previous one-byte Status revision.

On 2026-11-02 the owner reported **21 passed, 0 failed, 0 not completed** for the previous protocol-version-1 suite, followed by green CI. That evidence applies to the previous wire format only.

Before the four-byte Status change, on 2026-11-03 the owner reported the protocol-version-2 suite passing on real Pico hardware: **21 passed, 0 failed, 0 not completed; Overall: PASSED**. The owner separately reported 82 protocol host tests, 75 register host tests and full local verification with 462 tests and all 17 checks passing during this migration. These are distinct from hardware-suite results. UART hardware-error injection, real servo motion, calibration transfer, restart and watchdog are outside this fixture.

## Extended register suite

### Baseline before every scenario

| Action | TX | Expected RX |
| --- | --- | --- |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 1. Read ProtocolVersion 2

Suite case ID: `read_version`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Read register 0x00 | `01 00 02 00 FB DA CE AB` | `80 00 02 00 02 00 72 1F 4D 37` |

### 2. Read Status

Suite case ID: `read_status`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 3. Discard READ of unassigned register 0x02

Suite case ID: `unassigned_read`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Read former LastError index; no response | `01 02 04 00 13 A9 10 FE` | No reply; observe 200 ms |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 4. Read Control

Suite case ID: `read_control`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |

### 5. Read ServoCount

Suite case ID: `read_servo_count`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Read register 0x04 | `01 04 01 00 E4 21 EA 87` | `80 04 01 00 00 EF BA 64 F9` |

### 6. WRITE enable without calibration

Suite case ID: `enable_write`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 02 register 0x03 | `02 03 01 00 01 32 51 92 D8` | `81 03 01 00 06 D3 0E B0 B0` |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 7. SET enable without calibration

Suite case ID: `enable_set`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 03 register 0x03 | `03 03 01 00 01 82 78 F2 E5` | `81 03 01 00 06 D3 0E B0 B0` |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 8. Disable operations ACK after rejected enable

Suite case ID: `disable_after_rejection`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 03 register 0x03 | `03 03 01 00 01 82 78 F2 E5` | `81 03 01 00 06 D3 0E B0 B0` |
| Opcode 02 register 0x03 | `02 03 01 00 00 A4 61 95 AF` | `80 03 00 00 7E D7 5B CE` |
| Opcode 04 register 0x03 | `04 03 01 00 01 92 A4 D2 57` | `80 03 00 00 7E D7 5B CE` |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 9. WRITE reserved Control bits

Suite case ID: `reserved_write`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

Restart is not implemented; only Control bit 0 is currently valid.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 02 register 0x03 | `02 03 01 00 03 1E 30 9C 36` | `81 03 01 00 05 69 5F B9 29` |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 10. SET reserved Control bits

Suite case ID: `reserved_set`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

Restart is not implemented; only Control bit 0 is currently valid.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 03 register 0x03 | `03 03 01 00 02 38 29 FB 7C` | `81 03 01 00 05 69 5F B9 29` |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 11. CLR reserved Control bits

Suite case ID: `reserved_clr`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

Restart is not implemented; only Control bit 0 is currently valid.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 04 register 0x03 | `04 03 01 00 02 28 F5 DB CE` | `81 03 01 00 05 69 5F B9 29` |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 12. Discard writes to unassigned register 0x02

Suite case ID: `unassigned_write`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| WRITE former LastError index; no response | `02 02 04 00 01 00 00 00 4E EC B5 93` | No reply; observe 200 ms |
| WRITE zero to former LastError index; no response | `02 02 04 00 00 00 00 00 2B 8B 09 2B` | No reply; observe 200 ms |
| Control remains disabled | `01 03 01 00 61 37 A5 82` | `80 03 01 00 00 56 82 B3 64` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 13. WRITE to read-only version

Suite case ID: `readonly_version`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 02 register 0x00 | `02 00 02 00 EF BE 8B B9 3C A3` | `81 00 01 00 02 24 65 68 A5` |
| Read register 0x00 | `01 00 02 00 FB DA CE AB` | `80 00 02 00 02 00 72 1F 4D 37` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 14. WRITE to read-only status

Suite case ID: `readonly_status`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 02 register 0x01 | `02 01 04 00 FF FF FF FF 55 B1 5A C4` | `81 01 01 00 02 41 02 D4 1D` |
| Read register 0x01 | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 15. WRITE to read-only servo_count

Suite case ID: `readonly_servo_count`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Opcode 02 register 0x04 | `02 04 01 00 FF 90 B6 40 1F` | `81 04 01 00 02 73 F2 0A 2A` |
| Read register 0x04 | `01 04 01 00 E4 21 EA 87` | `80 04 01 00 00 EF BA 64 F9` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 16. Discard READ of unknown register 0xFF

Suite case ID: `unknown_read`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| READ unknown index; no response | `01 FF 01 00 D5 BD 7B 3E` | No reply; observe 200 ms |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 17. Discard implausible READ lengths

Suite case ID: `invalid_read_lengths`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| Discard READ index 00, length 1 | `01 00 01 00 38 89 E3 80` | No reply; observe 200 ms |
| Discard READ index 01, length 2 | `01 01 02 00 CC B0 0C AA` | No reply; observe 200 ms |
| Discard READ index 03, length 2 | `01 03 02 00 A2 64 88 A9` | No reply; observe 200 ms |
| Discard READ index 04, length 0 | `01 04 00 00 A5 10 F1 9E` | No reply; observe 200 ms |
| Discard READ index 20, length 3 | `01 20 03 00 5A AD 98 8A` | No reply; observe 200 ms |
| Discard READ index 31, length 8 | `01 31 08 00 D6 BD 88 74` | No reply; observe 200 ms |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 18. Reject plausible target access with ServoCount zero

Suite case ID: `targets_without_servos`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

The static target region is plausible; the configured count is zero. These accesses reach the service and receive NACK.

| Action | TX | Expected RX |
| --- | --- | --- |
| READ target 0 returns OutOfBounds | `01 20 04 00 9D 3B D9 C5` | `81 20 01 00 04 2F 6F 39 EC` |
| Opcode 02 register 0x20 | `02 20 04 00 00 00 00 00 9E A3 2B 73` | `81 20 01 00 04 2F 6F 39 EC` |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 19. Discard unknown indices and implausible write lengths

Suite case ID: `discarded_write_headers`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| WRITE unknown index; no response | `02 FF 01 00 55 C0 6D 24 78` | No reply; observe 200 ms |
| SET unassigned index; no response | `03 02 04 00 FF FF FF FF 56 AB 18 39` | No reply; observe 200 ms |
| CLR unassigned index; no response | `04 02 04 00 FF FF FF FF 4F A2 DD 33` | No reply; observe 200 ms |
| WRITE target with misaligned length; no response | `02 20 03 00 FF FF FF A3 97 41 4A` | No reply; observe 200 ms |
| SET Status with length 256; header only is sufficient to reject | `03 01 00 01` | No reply; observe 200 ms |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 20. Ignore damaged CRC and accept next request

Suite case ID: `bad_crc`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

| Action | TX | Expected RX |
| --- | --- | --- |
| READ Status with corrupted CRC | `01 01 04 00 4A 17 56 7C` | No reply; observe 200 ms |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

### 21. Discard timed-out header before its old CRC arrives

Suite case ID: `incomplete_timeout`. Hardware-confirmed after the four-byte Status change on 2026-11-03.

Observe 200 ms after each partial transmission, exceeding the firmware idle timeout of 100 ms.

| Action | TX | Expected RX |
| --- | --- | --- |
| Send only READ Status header | `01 01 04 00` | No reply; observe 200 ms |
| Send only old CRC after timeout | `4A 17 56 FC` | No reply; observe 200 ms |
| Status remains zero | `01 01 04 00 4A 17 56 FC` | `80 01 04 00 00 00 00 00 11 13 FF 0B` |

## Reproduce wire bytes independently

CRC-32/ISO-HDLC uses reflected polynomial 0xEDB88320, initial register 0xFFFFFFFF and final XOR 0xFFFFFFFF; the standard check value for ASCII `123456789` is 0xCBF43926. CRC covers header and payload, excluding the CRC itself. Python's `zlib.crc32` with its default argument calculates this variant directly.

This example generates request and ACK bytes without hardware access:

```python
import struct
import zlib

body = bytes.fromhex("80 00 02 00 02 00")
wire = body + struct.pack("<I", zlib.crc32(body))
print(wire.hex(" ").upper())
```

Each JSON step retains request/response bodies alongside complete wire bytes. The runner checks their CRC-derived consistency before opening the serial port. `schema_version=1` describes the JSON format and is independent of protocol version 2.
