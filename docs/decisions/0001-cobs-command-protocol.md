# 0001: COBS command protocol for the servo controller

Date: 2026-11-05

Status: Agreed design direction; External read-only calibration EEPROM selected; remaining wire details and timing rules are provisional. Not implemented. Existing firmware and UART tools still use the register protocol.

## Context

The existing parser scans for plausible headers after corruption. A throughput experiment also encountered an unexplained response timeout. Simpler framing should provide explicit boundaries and bounded recovery, but does not establish the cause of that timeout or prevent UART overruns.

## Decision

Use a command protocol independent of UART transport details. Start with UART; keep higher layers hardware independent. Each transmitted frame is:

```text
00 | COBS(command | command-specific data | CRC32) | 00
```

Both zero bytes are frame delimiters: a start delimiter and an end delimiter. They do not provide electrical or bit synchronization; UART synchronizes each character using its start bit. COBS keeps zero out of the encoded content. The leading delimiter establishes a fresh frame boundary even if a previous transmission was interrupted without its end delimiter. Ignore consecutive delimiters.

CRC-32/ISO-HDLC covers command and command-specific data before encoding, excluding the CRC itself. CRC byte order is little endian. Retain the existing software/hardware checksum abstraction. Use the same framing and CRC for requests, ACK and NACK.

Maximum decoded content, including command and CRC, is 254 bytes: one command byte, up to 249 command-data bytes, four CRC bytes. Reserve up to 256 encoded bytes and 258 bytes including both delimiters. This accommodates an encoder emitting an empty terminal code block at the 254-byte boundary. The initial proposal of a 254-byte application payload was superseded by this total decoded limit.

There is no explicit frame-length field. After COBS decoding, check general size bounds (at least command plus CRC, at most 254 bytes), then CRC, then command-specific shape. A CRC-valid request with an invalid command-specific length receives NACK. Never execute an action before integrity and semantic validation succeed. Ignore empty delimiter runs. At startup ignore bytes until a zero establishes a frame boundary. On timeout or overflow discard the partial frame and ignore bytes until the next zero. A malformed completed frame is discarded; its ending zero also establishes a new boundary. Receive timeout duration remains to be selected.

Corrupt COBS/CRC frames receive no response. Structurally intact, CRC-valid requests with unknown commands or invalid parameters receive NACK. The master keeps at most one request outstanding.

## Proposed commands and replies

Numeric assignments below are provisional:

| Code | Command | Command data |
| --- | --- | --- |
| 0x01 | GET_STATUS | None |
| 0x02 | OUTPUT_ENABLE | One byte, 0 or 1 |
| 0x03 | SET_TARGETS | Start ServoId u8 followed by consecutive i32 centidegree angles |

Use little endian for multibyte values. SET_TARGETS requires at least one angle, valid range and valid angles throughout. Reject a complete batch on any failure; preserve omitted servo targets. Prefer complete poses for normal walking, while retaining consecutive partial updates.

Replies:

```text
ACK  (0x80) | original command | optional result data
NACK (0x81) | original command | error code u8
```

GET_STATUS returns the u32 status flags as four little-endian result bytes. OUTPUT_ENABLE and SET_TARGETS return ACK with only the original command and no result data. There are no calibration messages or sequence numbers. Error-code assignments remain to be finalized. Status has no separate last-command error: NACK explains rejected requests; status error flags describe latched controller failures.

Remove register addressing, READ/WRITE/SET/CLR opcodes, ProtocolVersion and ServoCount queries, and target/calibration readback. The master owns its robot configuration. Range validation catches out-of-range requests but cannot detect every mismatch in intended servo count. Calibration decoding, CRC validation and angle conversion remain useful. Persistent storage is read-only from the servo firmware; the flash journal is not part of this proposed operating path.

## Output lifecycle and status

OUTPUT_ENABLE(1) grants permission but produces no pulses by itself. A first SET_TARGETS must contain all configured servos, starting at zero, and starts output with those targets. Once running, partial consecutive updates are allowed. OUTPUT_ENABLE(0) stops output and clears permission. A new enable/start cycle requires a new full initial target set. No implicit zero-degree startup pose. Repeated enable while already enabled or running is acknowledged without restarting or changing targets. Repeated disable succeeds. SET_TARGETS while disabled is rejected. Enable requires a complete valid boot-loaded calibration and no latched controller error. Disable remains allowed in an error state; ACK for disable requires confirmed output shutdown. A failed shutdown must not falsely clear RUNNING.

The proposed status is a u32 bitmask:

| Bit | Flag |
| --- | --- |
| 0 | CALIBRATION_VALID |
| 1 | OUTPUT_ENABLED |
| 2 | RUNNING |
| 8 | OUTPUT_ERROR |

Other bits are reserved. RUNNING reports confirmed signal generation, not measured servo position. OUTPUT_ENABLED is permission only: after enable, RUNNING stays clear until a complete valid initial target set starts output. After confirmed disable, both flags are clear. Do not clear RUNNING solely because permission was revoked if shutdown failed.

OUTPUT_ERROR latches until restart and blocks startup. An output failure must trigger the existing safe shutdown path; report confirmed output state, and do not claim Low outputs if shutdown fails. CALIBRATION_VALID is set only after complete boot-time validation. Both unreadable EEPROM and invalid calibration leave this flag clear; detailed causes are reported through debug UART rather than separate protocol error flags.

There is no BUSY status flag. Calibration is never written or reloaded during operation, so no runtime persistence state or completion result is needed. Temporary inability to accept a new output plan is reported directly as NACK(Busy), not as a latched output error.

Accepted active updates must be applied together at a signal-cycle boundary. ACK confirms acceptance, not physical position or guaranteed already-active pulses. If a previous signal plan is still pending and a new one cannot be accepted, return NACK(Busy), preserve accepted targets, and do not latch a controller error. Lost input must not create a queue of stale movement commands. Link-loss behavior remains a separate decision; disabling servo holding torque is not assumed to be safe for a standing robot.

## Calibration outside the operating protocol

Use an external I2C EEPROM programmed and verified at a separate calibration station. The robot firmware reads the complete dataset during boot, validates format/version, data length, CRC32, servo count and every calibration entry, then installs an immutable in-RAM calibration for that run. No physical output or implicit motion is started during loading.

If the EEPROM cannot be read or data is invalid/incomplete, keep output disabled and inactive and clear CALIBRATION_VALID. Reject enable and target requests, but keep status queries and disable available after initialization. Do not silently fall back to an old flash journal or a default zero-degree calibration. No automatic polling/reloading or hot replacement is planned; change data only through the powered-off procedure below.

The calibration replacement procedure is mandatory:

1. Shut down and fully power off the robot/controller, including independent USB supplies.
2. Disconnect the EEPROM from the controller and connect it to the separate calibration station.
3. Write and read back the complete dataset; validate it before use.
4. Disconnect the station and reconnect the EEPROM to the unpowered controller.
5. Power up the system; load and validate the complete dataset before allowing output.

Do not program an EEPROM in place while the controller is powered, hot-swap it, or reload calibration during operation. The station may power the detached EEPROM for programming. There is no protocol command for calibration, reload or calibration-triggered restart.

The external station owns EEPROM writes, readback verification and dataset backups. Provide hardware write protection in the robot where supported. Exact EEPROM type, addressing, pull-ups, programming connector and write-protect wiring must be selected and reviewed separately. Prevent programmer/Pico bus contention and unintended power through signal pins. Do not add a unique controller ID at this stage; physical labeling and correct servo assignment remain necessary.

WRITE_CALIBRATION, transport fragments, sequence checks, duplicate fragment handling, transfer timeout, flash write/erase retries, persistence completion and calibration-triggered restart are removed from the new servo protocol. The station can reuse the existing calibration format and CRC32; its final EEPROM layout needs a separate storage decision. Existing flash code and hardware tests remain as independent learning/verification work, not deleted by this decision.

## GPIO feasibility and hardware impact

A provisional allocation fits the current 18-servo requirement on the Pico's 26 exposed GPIOs:

| Function | GPIOs |
| --- | --- |
| 16 servo outputs | GP0..GP15 |
| Protocol UART0 | GP16 TX, GP17 RX |
| EEPROM I2C1 | GP18 SDA, GP19 SCL |
| Debug UART1 | GP20 TX, GP21 RX |
| Two servo outputs | GP22, GP26 |
| Two reserved analog measurements | GP27, GP28 |

PIO masks permit nonconsecutive servo pins. This allocation uses all 26 pins; there is no simultaneous dedicated SPI interface or spare GPIO reservation. The benchmark jumpers on GP22/GP26 are test-only and incompatible with using those pins as servo outputs in the operating firmware. No GPIO expander or second MCU is needed for this allocation.

The existing hardware proposal routes 18 servo signals as GP0..GP17. It therefore conflicts with protocol UART0 on GP16/17 and must be revised to route the final two servo channels to GP22/GP26. No KiCad or physical wiring is changed by this decision document. ADC measurement circuits are separate work.

References: [Pico GPIO/peripheral resources](https://www.raspberrypi.com/documentation/microcontrollers/pico-series.html), [example EEPROM with hardware write protection](https://ww1.microchip.com/downloads/aemDocuments/documents/MPD/ProductDocuments/DataSheets/24AA256-24LC256-24FC256-256K-I2C-Serial-EEPROM-DS20001203.pdf). The referenced EEPROM family is an example, not a selected component.

## Alternatives and consequences

Keep register protocol: less migration, but more general access/header validation and less explicit boundary recovery. CRC16: saves two bytes but reduces detection margin; retain CRC32. Internal flash calibration storage: avoids a component and two I2C pins but requires flash/RAM execution safety, persistence recovery and a writable calibration service. External EEPROM adds hardware but removes those tasks from the operating firmware when programmed separately.

COBS eliminates opcode scanning inside damaged payloads and allows bounded frame buffers. It does not guarantee delivery, correct servo mounting, measured position or recovery from arbitrary hardware failure. External boot-only calibration removes runtime transfer and flash persistence states. The separate calibration station still requires reliable writes and verification. PIO/DMA lifecycle and physical safety constraints remain unchanged. EEPROM CRC cannot identify a valid dataset assigned to the wrong physical servos.

## Verification plan

Before hardware migration, test COBS boundary cases (zeros, 253/254-byte inputs, terminal blocks), size limits, CRC vectors, fragmentation of serial reads, consecutive delimiters, noise, byte loss/insertion, corrupted/lost delimiters, overflow and timeout recovery. Test all-or-nothing target validation, pending-plan rejection without latching a fault, enable/start/stop lifecycle and repeated enable/disable. Test EEPROM read failures, malformed/incomplete/CRC-invalid datasets, servo-count mismatch and rejection of output without valid calibration. Confirm no fallback to flash or default calibration. Verify the programming station separately. Then adapt UART suites and the retained throughput benchmark. No new-protocol tests or hardware verification are claimed by this document.
