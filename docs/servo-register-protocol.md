# Servo register protocol — version 2

Current development status (2026-11-03): calibration WRITE at 0x10 now validates and owns a complete eighteen-servo dataset in RAM. A pending request sets Busy; completion records its result and releases the request. The owner confirmed all 12 request tests and full local verification with **489 tests and all 17 checks passing** (102 register-package tests in total). Physical ACK-completion gating, journal scheduling and restart integration remain future work. No hardware calibration-transfer run or CI result is claimed for this change. The preceding four-byte Status revision passed its UART suite and CI.

Current development contract, updated 2026-11-03. ProtocolVersion is **2**, independent of the Cargo package and firmware versions. This revision replaces version-1 READ replies and LastError with ACK/NACK. It is not a released compatibility promise.

## Purpose and scope

The servo controller is a subordinate service. The master initiates requests; the controller sends responses only, never unsolicited protocol events. Diagnostic logging uses a separate UART. UART is the current transport; the framing and register layers remain hardware-independent. SPI/I2C adapters are not implemented.

The concrete hardware-test path is PC -> UART0 -> UartTransport -> FrameProcessor -> RegisterService -> NoServoOutput. The normal Pico demo does not yet run this protocol service with real servo outputs or journal-loaded calibration.

## Internal memory versus wire representation

Register indices are logical u8 values, not raw addresses. Internal alignment does not dictate wire widths. Header fields and multi-byte data use little endian. Target angles are signed i32 centidegrees; errors are u8. Fixed registers use the explicit widths below. Status is u32 and transfers exactly four little-endian bytes; Busy and calibration-save result semantics are implemented in the register service.

## Working register map

| Index | Register | Current wire bytes | Access |
| --- | --- | ---: | --- |
| 0x00 | ProtocolVersion; UART fixture reports 2 | 2 | READ |
| 0x01 | Status | 4 | READ |
| 0x02 | Unassigned; reserved for future restart reason | — | None |
| 0x03 | Control | 1 | READ, WRITE, SET, CLR |
| 0x04 | ServoCount | 1 | READ |
| 0x10 | Complete eighteen-servo calibration request | 368 | WRITE; READ/SET/CLR rejected |
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

Current Status bits are CALIBRATION_VALID=0, OUTPUT_ACTIVE=1 and FAULT=3. Bit 2 is Busy while a calibration request is pending. Bits 8..15 contain its completion result. Status reports actual backend logical state, including after an error, rather than measured position or power. Invalid angles and incomplete targets do not set a technical Fault; backend failures latch Fault and preserve accepted control/target values as specified by the backend contract.

Target reads return accepted backend values. Inactive writes prepare targets; active writes submit an atomic complete candidate batch. Omitted targets retain their accepted values. No partial batch is accepted if a value fails validation. A successful ACK does not guarantee that a pending signal plan is already active.

## UART parsing and resynchronization

StreamParser owns a fixed 520-byte buffer. It first checks opcode direction, then request index/length plausibility independently of access permissions. Current regions all support READ, so a synthetic READ header supplies that plausibility check. The original opcode is then checked for access rights. Responses instead use ACK/NACK framing rules without register-map validation.

Rejected candidates advance one byte, preserving overlapping possible starts. After a CRC error the length cannot be trusted, so the parser searches again rather than dropping a presumed whole frame. Plausibility filtering rejects false candidates such as SET Status with Length=256. A corrupt candidate with a plausible index and length can still delay recovery until timeout; immediate recovery is not guaranteed for arbitrary noise.

push rejects BufferFull without accepting or overwriting a byte. next_frame returns a borrowed payload; its bytes are retired on the next mutable parser use. Drain complete frames before on_timeout resets an incomplete remainder. Transport error handling and clock/idle detection belong to the caller.

## Byte transport and queued responses

ByteTransport is nonblocking. try_read returns a byte or None; try_write returns true only for an accepted byte. Backpressure and errors preserve unsent bytes. Acceptance by a UART FIFO does not mean that the final stop bit has left the wire.

ResponseBuffer owns one copied encoded frame. FrameProcessor services the output backend every poll, drains pending responses using a byte budget, then processes complete requests when response storage is available. Every service-produced ACK/NACK is queued before more requests are consumed. A newly queued response is transmitted on a later poll. Deferred timeouts preserve complete buffered requests and pending responses.

## Calibration request acceptance and completion

Register 0x10 accepts exactly 368 payload bytes: the existing four-byte format header, eighteen 20-byte entries and inner CRC32. The outer protocol CRC remains separate. Unknown indices and implausible lengths are still discarded by the stream parser; direct service calls can return InvalidLength NACK. A shorter partial calibration does not reach the wire service for an InvalidValue reply. The register map permits READ for structural plausibility, but the service rejects calibration READ, SET and CLR with UnsupportedAccess.

Acceptance requires inactive actual output and no pending request. Busy or active output returns InvalidState; invalid format, dataset CRC, duplicate/missing IDs or invalid calibration values return InvalidValue when the frame length is plausible. The service reserves NACK response capacity before accepting anything. It decodes with SoftwareCrc32 through PendingCalibration's Checksum interface; the RAM type itself remains checksum-adapter independent.

Success installs the owned PendingCalibration<18>, resets the prior completion result and returns an empty ACK. Busy is derived from the presence of that request rather than a second mutable flag. Completeness currently uses the fixed eighteen-servo firmware capacity, not the active ServoCount, which may be zero before loading calibration. It does not update active calibration or ServoCount.

While Busy, otherwise valid modifying requests are rejected before target-count or backend processing. Ordinary READ operations remain available. Invalid requests do not replace the pending data or prior operation result. pending_calibration() returns an immutable reference without changing Busy. complete_calibration(result) records the result before removing the request; a call without a request returns NoPendingCalibration without changing state. After either success or failure the request is released; retry requires submitting a new complete dataset.

Status bits 8..15 use 0=None, 1=Success, 2=StorageError and 3=VerificationFailed. None is reported initially and after accepting a new request. The result persists after completion; rejected requests do not overwrite it. Fault remains an independent output-status bit.

This step does not write flash. The program loop must later wait for the ACK to finish physically on UART before invoking the journal writer. RegisterService exposes request and completion methods, but forwarding them through FrameProcessor and coordinating transport completion are still future work. An available pending request alone is not a write authorization.

## Planned persistence scheduling and restart — not implemented

Calibration request acceptance and Busy/result handling are implemented as described above. The future loop will send the queued ACK before starting persistence. Flash writing starts only after physical UART completion of that ACK. Completion records an action result before clearing Busy. Rejected requests must not overwrite the retained result of a longer action. Status polling remains the intended completion mechanism; flash access can temporarily pause processing.

The master will explicitly request restart after confirming successful persistence. Restart will require inactive output and no Busy; its ACK must finish on the wire before reset. Boot loads and validates calibration, leaving outputs inactive. A hardware watchdog is planned to reset a stalled main loop; its deadline must cover legitimate flash operations. Restart reason, restart control and watchdog integration are not implemented in this revision.

Calibration remains an off-robot procedure. Protocol persistence does not measure angles or prove correct assembly. Board IDs, mandatory authentication, custom USB firmware updates and partial calibration writes are outside the current design. Current register widths and missing regions must not be mistaken for implemented planned features.

## Verification record and limits

The current RAM-request step passed 489 local tests and all 17 checks as recorded above; no hardware calibration transfer or CI result is claimed yet. The following counts describe earlier protocol revisions.

On 2026-11-03 the owner reported **82 protocol host tests**, **75 register host tests**, and full local verification with **462 passed, 0 failed; all 17 checks passed**. The owner also reported **21 passing UART hardware scenarios** for protocol version 2. See [current vectors and suite](uart-register-test-vectors.md) and the [hardware procedure](../firmware/pico/hardware_tests/README.md#uart-register-requestresponse-test).

The previous version-1 UART suite passed on 2026-11-02; its bytes do not establish version-2 compatibility. CI for the current ACK/NACK revision has not yet been reported. Host tests, no-servo UART hardware tests and real servo output tests are separate evidence. Physical UART error injection, arbitrary EMI recovery, protocol-to-real-PIO output, journal-backed runtime initialization and longer-operation completion remain unverified in this combined path.
