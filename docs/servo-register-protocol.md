# Servo register protocol — version 2

Current development change: Status now transfers four little-endian bytes. Bits 8..15 are reserved for the result of the last longer operation; Busy and result handling are not implemented. On 2026-11-03 the owner confirmed all 78 register host tests and all 21 revised UART hardware scenarios passing. Full local verification passed 465 tests and all 17 checks; CI for the width change has not yet been reported; older totals below predate it.

Current development contract, updated 2026-11-03. ProtocolVersion is **2**, independent of the Cargo package and firmware versions. This revision replaces version-1 READ replies and LastError with ACK/NACK. It is not a released compatibility promise.

## Purpose and scope

The servo controller is a subordinate service. The master initiates requests; the controller sends responses only, never unsolicited protocol events. Diagnostic logging uses a separate UART. UART is the current transport; the framing and register layers remain hardware-independent. SPI/I2C adapters are not implemented.

The concrete hardware-test path is PC -> UART0 -> UartTransport -> FrameProcessor -> RegisterService -> NoServoOutput. The normal Pico demo does not yet run this protocol service with real servo outputs or journal-loaded calibration.

## Internal memory versus wire representation

Register indices are logical u8 values, not raw addresses. Internal alignment does not dictate wire widths. Header fields and multi-byte data use little endian. Target angles are signed i32 centidegrees; errors are u8. Fixed registers use the explicit widths below. Status is u32 and transfers exactly four little-endian bytes; Busy and action-result semantics are not implemented yet.

## Working register map

| Index | Register | Current wire bytes | Access |
| --- | --- | ---: | --- |
| 0x00 | ProtocolVersion; UART fixture reports 2 | 2 | READ |
| 0x01 | Status | 4 | READ |
| 0x02 | Unassigned; reserved for future restart reason | — | None |
| 0x03 | Control | 1 | READ, WRITE, SET, CLR |
| 0x04 | ServoCount | 1 | READ |
| 0x10 | Planned complete calibration block; not implemented | — | Not yet available |
| 0x20..0x31 | Target angles, servo IDs 0..17 | 4 per entry | READ, WRITE |

The map describes capacity for eighteen targets; the service also enforces its configured servo count. The UART no-servo fixture reports zero. Broader capacities and final GPIO assignments remain future configuration work. No zero-degree defaults are imposed on the real ServoBank: required targets must be prepared before startup.

## Operations and frames

```text
Opcode:u8 | Register:u8 | Length:u16 LE | Payload | CRC32 LE
```

| Opcode | Value | Payload |
| --- | --- | --- |
| READ | 0x01 | None; Length specifies requested response data |
| WRITE | 0x02 | Exactly Length bytes |
| SET | 0x03 | Exactly Length bytes; bitwise OR |
| CLR | 0x04 | Exactly Length bytes; clear selected bits |
| ACK | 0x80 | READ data, or empty after a successful modification |
| NACK | 0x81 | Exactly one error byte; Length=1 |

READ/WRITE/SET/CLR are request-only; ACK/NACK are response-only. ACK and NACK preserve the original requested index. An empty ACK has Length=0. Register permissions do not apply to response parsing: an empty write ACK is valid independently of register width.

CRC-32/ISO-HDLC covers the complete header and payload. It is serialized as four little-endian bytes. The reflected polynomial is 0xEDB88320, initial register and final XOR are 0xFFFFFFFF, and the ASCII `123456789` check value is 0xCBF43926. Every calculation starts independently. The implementation receives its checksum function from the caller.

Maximum payload is 512 bytes; maximum frame is 520 bytes. READ requests and empty ACKs are eight bytes. NACKs are nine bytes. An eighteen-target WRITE is 80 bytes and its success ACK is eight bytes. The frame codec validates size, direction and exact payload length before modifying an output buffer.

There is no start marker, request ID or automatic retransmission. The master keeps at most one request outstanding and waits for its response or timeout before retrying. Read ProtocolVersion at connection setup. A version mismatch must not be treated as permission to send version-dependent control requests.

## Replies and errors

Unknown indices and implausible lengths are rejected by the stream parser without a reply. This includes zero lengths, incorrect register-width alignment, ranges crossing a region and requested lengths exceeding the frame cap. The master observes a timeout for these cases.

Known indices with plausible lengths are received completely and CRC-checked even if their operation is prohibited. Accepted requests receive ACK; rejected accesses, values or controller states receive NACK. Bad CRC and incomplete frames produce no response. No hardware action is permitted before full frame and semantic validation.

| Error | Value |
| --- | ---: |
| None | 0 |
| UnknownIndex | 1 |
| UnsupportedAccess | 2 |
| InvalidLength | 3 |
| OutOfBounds | 4 |
| InvalidValue | 5 |
| NotConfigured | 6 |
| InvalidState | 7 |
| OutputError | 8 |

The direct RegisterService interface revalidates requests, so it can report UnknownIndex or InvalidLength as NACK to a direct caller. Through the stream parser, structurally implausible headers are discarded instead. OutOfBounds still applies to plausible target accesses exceeding the configured count. LastError no longer exists; an ACK reports acceptance, not a measured physical servo position.

Caller-side response-buffer errors are ServiceError rather than wire NACKs. Modifying operations reserve one byte of response capacity before executing, so they can report a backend failure without losing its result. A short caller buffer must not cause partial state changes.

## Control, status and operation rules

Only Control bit 0 (OUTPUT_ENABLE) is implemented. WRITE replaces the value, SET applies OR and CLR clears bits. Reserved bits are rejected even when clearing them would have no effect. Enable requires valid calibration and no latched Fault. Disable remains possible in Fault and does not clear it.

Current Status bits are CALIBRATION_VALID=0, OUTPUT_ACTIVE=1 and FAULT=3. Bit 2 is reserved for future Busy. Status reports actual backend logical state, including after an error, rather than measured position or power. Invalid angles and incomplete targets do not set a technical Fault; backend failures latch Fault and preserve accepted control/target values as specified by the backend contract.

Target reads return accepted backend values. Inactive writes prepare targets; active writes submit an atomic complete candidate batch. Omitted targets retain their accepted values. No partial batch is accepted if a value fails validation. A successful ACK does not guarantee that a pending signal plan is already active.

## UART parsing and resynchronization

StreamParser owns a fixed 520-byte buffer. It first checks opcode direction, then request index/length plausibility independently of access permissions. Current regions all support READ, so a synthetic READ header supplies that plausibility check. The original opcode is then checked for access rights. Responses instead use ACK/NACK framing rules without register-map validation.

Rejected candidates advance one byte, preserving overlapping possible starts. After a CRC error the length cannot be trusted, so the parser searches again rather than dropping a presumed whole frame. Plausibility filtering rejects false candidates such as SET Status with Length=256. A corrupt candidate with a plausible index and length can still delay recovery until timeout; immediate recovery is not guaranteed for arbitrary noise.

push rejects BufferFull without accepting or overwriting a byte. next_frame returns a borrowed payload; its bytes are retired on the next mutable parser use. Drain complete frames before on_timeout resets an incomplete remainder. Transport error handling and clock/idle detection belong to the caller.

## Byte transport and queued responses

ByteTransport is nonblocking. try_read returns a byte or None; try_write returns true only for an accepted byte. Backpressure and errors preserve unsent bytes. Acceptance by a UART FIFO does not mean that the final stop bit has left the wire.

ResponseBuffer owns one copied encoded frame. FrameProcessor services the output backend every poll, drains pending responses using a byte budget, then processes complete requests when response storage is available. Every service-produced ACK/NACK is queued before more requests are consumed. A newly queued response is transmitted on a later poll. Deferred timeouts preserve complete buffered requests and pending responses.

## Planned longer operations — not implemented

Calibration transfer will accept a complete validated dataset only while output is inactive and Busy is clear. After buffering, Busy is set and ACK is sent. Flash writing starts only after physical UART completion of that ACK. Completion records an action result before clearing Busy. Rejected requests must not overwrite the retained result of a longer action. Status polling remains the intended completion mechanism; flash access can temporarily pause processing.

The master will explicitly request restart after confirming successful persistence. Restart will require inactive output and no Busy; its ACK must finish on the wire before reset. Boot loads and validates calibration, leaving outputs inactive. A hardware watchdog is planned to reset a stalled main loop; its deadline must cover legitimate flash operations. Restart reason, action-result fields, Busy, restart control and watchdog integration are not implemented in this revision.

Calibration remains an off-robot procedure. Protocol persistence does not measure angles or prove correct assembly. Board IDs, mandatory authentication, custom USB firmware updates and partial calibration writes are outside the current design. Current register widths and missing regions must not be mistaken for implemented planned features.

## Verification record and limits

On 2026-11-03 the owner reported **82 protocol host tests**, **75 register host tests**, and full local verification with **462 passed, 0 failed; all 17 checks passed**. The owner also reported **21 passing UART hardware scenarios** for protocol version 2. See [current vectors and suite](uart-register-test-vectors.md) and the [hardware procedure](../firmware/pico/hardware_tests/README.md#uart-register-requestresponse-test).

The previous version-1 UART suite passed on 2026-11-02; its bytes do not establish version-2 compatibility. CI for the current ACK/NACK revision has not yet been reported. Host tests, no-servo UART hardware tests and real servo output tests are separate evidence. Physical UART error injection, arbitrary EMI recovery, protocol-to-real-PIO output, journal-backed runtime initialization and longer-operation completion remain unverified in this combined path.
