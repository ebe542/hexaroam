# Servo register protocol — first draft

Status: design draft recorded on 2026-10-30, updated on 2026-10-31; initial framing/validation implementation with 50 owner-reported passing host tests; register service and UART integration remain pending. This document records the agreed direction and identifies remaining choices before implementation. Numeric addresses, wire-field widths and bit assignments below are the current working proposal, not a released compatibility promise.

## Purpose and scope

The servo controller is a subordinate service. One protocol interface handles target angles, status, calibration transfer and invalidation. A separate UART carries diagnostic logs; log text never appears in the protocol stream. The main board initiates all transactions. The servo controller sends only READ responses, including READ error responses; it sends no unsolicited protocol events or WRITE acknowledgements.

Firmware is installed through USB. There is no custom firmware-update protocol, mandatory board serial number, authentication scheme or production provisioning workflow. Correct assembly and a calibration set appropriate to that assembly are assumptions. Format validation and CRC do not identify swapped servos or prove mechanical correctness.

The logical register operations are transport-independent. UART initially supplies a byte stream; SPI/I2C adapters would need their own transaction and response-delivery handling. Their electrical timing and addressing are not defined here. Start with UART at the established 115200 baud, 8N1; a different rate can be selected and verified later. No transport driver or main-loop changes are part of this draft.

## Internal memory versus wire representation

Internal fields and block starts are four-byte aligned. A small logical field may occupy four bytes internally but use fewer bytes on the wire. Internal padding is not transmitted and must not be interpreted as protocol data. The register space is logical: a received address is never a raw CPU/flash pointer and does not grant unrestricted memory access.

The address identifies a field or the start of an allowed array/block access. A descriptor maps it to internal storage and defines permissions, wire width, signedness and length constraints. A universal pointer calculation is not the public protocol contract. A scalar WRITE replaces the complete decoded value rather than preserving unspecified upper bytes.

The first draft uses fixed wire widths per field, not value-dependent Varints or ZigZag. All multibyte integer values are little endian. Angles remain signed i32 centidegrees (hundredths of a degree).

## Working register map

Addresses are logical indices, not byte offsets. The concrete map is subject to final review before freezing test vectors.

| Index | Field / region | Wire representation | Access |
| --- | --- | --- | --- |
| 0x00 | ProtocolVersion | 2 bytes; version-number convention to finalize | READ |
| 0x01 | Status | 1 byte | READ |
| 0x02 | LastError | 4-byte unsigned error code | READ; WRITE of zero clears |
| 0x03 | Control | 1 byte | READ, WRITE, SET, CLR |
| 0x04 | ServoCount | 1 byte | READ |
| 0x10 | Calibration block start | Complete existing serialized calibration format | READ, complete WRITE |
| 0x20 + ID | TargetAngle array | 4 bytes per signed angle | READ, WRITE |

For the current eighteen-servo board, target indices are 0x20 through 0x31. A WRITE starting at 0x25 with length 12 updates servos 5, 6 and 7. Length must be positive and divisible by four and the entire access must remain in the configured array. A scalar's length must match its defined wire width exactly. Length zero and cross-region accesses are rejected. Contiguous transfers of mixed scalar fields are not initially supported. SET/CLR apply only to Control, not to angles or calibration bytes.

The existing calibration format is 8 + 20 * ServoCount bytes for a complete set: 368 bytes for eighteen servos. It contains its existing format header and CRC. Reuse it unchanged; the protocol's outer CRC protects opcode, address, length and the complete serialized payload. The journal envelope, sequence number and 512-byte slot padding are internal and are not part of this transferred calibration block. See the [storage format](../firmware/servo-storage/README.md).

Protocol calibration writes require all configured servo IDs exactly once and valid entries. Partial calibration and per-servo invalidation are not exposed. Internally, a newer empty journal record with valid CRC invalidates the entire set; older complete records must not become authoritative again. A stored partial configuration is not sufficient to enable operation.

The no-valid-calibration READ outcome and the final pulse/backend compatibility checks still need a concrete error code definition.

The internal block orders fixed scalar fields first, then the four-byte-aligned calibration region and finally the target-angle array. Logical index 0x10 identifies the whole calibration block: its byte length does not consume following logical indices. Physical internal offsets are separate from protocol indices. The target array is the final logical region and can grow within the u8 index space.

## Operations and frames

Proposed header fields:

| Field | Bytes | Meaning |
| --- | ---: | --- |
| Opcode | 1 | READ=1, WRITE=2, SET=3, CLR=4 |
| Address | 1 | Logical starting index (u8) |
| Length | 2 | Payload bytes requested or supplied, not register count |

A four-byte header is followed by payload where applicable and a four-byte CRC-32/ISO-HDLC. CRC covers the header and all payload bytes in wire order, excluding the transmitted CRC itself. The CRC value is transmitted little endian. Reuse the project's existing checksum implementation and standard check vector.

```text
READ request:  READ  | Address | Length           | CRC32
READ response: READ  | Address | Length | Data    | CRC32
WRITE:         WRITE | Address | Length | Data    | CRC32
SET:           SET   | Address | Length | Bitmask | CRC32
CLR:           CLR   | Address | Length | Bitmask | CRC32
```

All operations share the header. A READ request has no payload, although Length specifies the requested response payload size. A READ response has exactly Length payload bytes. The main-board receiver and controller receiver therefore know their respective frame direction; they cannot infer READ payload presence from the opcode alone. Initial SET/CLR masks are one byte with Length=1.

There is no separate start marker, frame type, request ID, per-frame protocol version or automatic retransmission in this draft. ProtocolVersion is read during connection setup and is separate from firmware version; a stable discovery/framing convention is required. At most one READ transaction is outstanding. The controller processes operations in received order; the master must not mix unrelated requests while awaiting a READ response.

READ requests are eight bytes. WRITE/SET/CLR and READ responses are eight bytes plus Length. Eighteen contiguous angles require 72 payload bytes and an 80-byte WRITE. An eighteen-servo calibration WRITE is 376 bytes. These counts are for the proposed header and do not include idle time or UART start/stop bits.

## Replies and LastError

Successful READ replies repeat the requested address and length with the selected data. An accepted or rejected WRITE/SET/CLR produces no immediate response. The master can read state or LastError explicitly.

For a valid but rejected READ, the controller records the error and replies with the ordinary LastError address:

```text
READ | Address(LastError) | 4 | ErrorCode(u32) | CRC32
```

LastError is a normal register, not an address outside the register space. A master expecting another address recognizes this as an error response. If LastError itself was requested with an invalid length/access, the canonical four-byte error reply still supplies the reason. A valid direct read of LastError uses the same format normally.

No response means the master's READ transaction eventually times out. Corrupt or incomplete frames do not produce replies, since their opcode/address cannot be trusted. No register action occurs before full frame validation. Whether transport errors should also latch a diagnostic counter/error is still open.

LastError retains the most recently recorded rejected-operation code. Successful operations and reads do not clear it. WRITE of a four-byte zero clears it; nonzero writes are rejected. LastError does not identify a particular WRITE and is not an exactly-once acknowledgement. Reading an unchanged desired value establishes that the desired state exists, not that a particular preceding write was accepted. The master can clear LastError before a deliberate operation and read it afterwards, but this does not turn CRC failure or a lost WRITE into a positive acknowledgement.

An operational FAULT is distinct from LastError; clearing LastError does not recover servo output. Numeric error codes remain to be assigned. Expected categories include invalid address/length/access/state/value, incomplete calibration, busy, storage failure and output failure. The raw numeric reason exposed through a one-byte status summary versus LastError remains to be finalized if needed.

## Control, status and operation rules

Proposed Status bits (controller-owned, read-only):

| Bit | Meaning |
| ---: | --- |
| 0 | CALIBRATION_VALID |
| 1 | OUTPUT_ACTIVE |
| 2 | BUSY |
| 3 | FAULT |

Proposed Control bits:

| Bit | Meaning |
| ---: | --- |
| 0 | OUTPUT_ENABLE: requested output state |
| 1 | INVALIDATE_CALIBRATION: one-shot action request |

SET applies `control | mask`; CLR applies `control & !mask`; WRITE requests the complete control value. The resulting request must be permitted by state and reserved-bit rules before any side effect. Simultaneous enable and invalidation is rejected. A running invalidation cannot be cancelled through CLR. Firmware clears the action bit after successful completion; exactly how failure is reflected in this bit is still to be defined. Read-only/reserved bits cannot be changed by the master.

Targets initially default to zero internally, with output disabled. Ready-state writes prepare targets; active-state writes update them. Enabling output validates all configured targets against complete calibration: zero is not guaranteed to be in every servo's valid range. All outputs start as one complete plan. A target block is accepted atomically only after every value is valid and the downstream output accepts the whole candidate. The visible targets are accepted commands, not measured positions or evidence of mechanical movement.

Disabling output returns Active to Ready after successful stop; other inactive states retain their state, and Fault is not cleared. Invalidating first ends output and leaves the operating mode, clears the enable request, then durably appends an empty calibration record. Calibration replacement is permitted only while output is inactive, accepts a complete set into RAM, validates it and persists it using the existing journal/backend. A storage error must not automatically reactivate output. An error return is not a rollback guarantee; reload may be needed to determine durable state.

No valid/complete calibration means no servo signals; the controller stays available for status and configuration requests. A matching configuration can be supplied by the main board. Firmware applies local format, completeness, angle and supported-pulse validation; physical correctness remains an assembly/calibration responsibility.

Boot and service-state transitions, BUSY lifetime, write completion polling, and the previously intended restart after calibration changes require a precise follow-up decision. In particular, polling a BUSY bit cannot work while synchronous flash operations block request processing; the master must allow those operations to finish and refrain from sending data during the XIP-off interval. No timeout duration or timing guarantee is assigned here.

## UART parsing and resynchronization

Known opcode bytes are candidate starts, not unique markers: their values may appear anywhere in binary payloads or CRC. No command may execute just because an opcode or plausible header was seen.

1. Search for a known opcode.
2. Collect the fixed header and check opcode/address/access/length plausibility. Impossible candidates are rejected immediately without buffering their claimed payload or calculating a full-frame CRC.
3. For a plausible header, collect the exact frame size for the receiver's direction. Do not restart at opcode-like payload bytes.
4. Validate CRC, then apply value/state checks and execute or return a valid READ error.
5. On header rejection or CRC failure, search remaining buffered bytes and subsequent bytes for another candidate. On timeout, discard the remaining incomplete input after draining available frames. Advance by at least one byte for each rejected candidate so scanning always progresses; retain overlapping possible starts rather than dropping an entire untrusted length.

Use bounded buffers and length limits before trusting received lengths. Address/range addition must not overflow. A READ for an invalid address/length can be recognized as a complete header-plus-CRC request and checked before producing its error reply; candidate scanning must not emit replies from unvalidated data. For malformed non-READ headers with impossible claimed payload sizes, resynchronize without waiting for those payloads. The implemented parser returns CRC-valid READ requests with their ValidationError where applicable. An oversized requested READ length still needs only an eight-byte request; it cannot cause a large payload allocation. Unsupported response opcodes and implausible non-READ candidates are skipped before payload collection.

An inter-byte receive timeout discards an incomplete candidate and resets collection; an idle gap is an additional reset aid, not the sole means of recovery. Header plausibility and CRC validation are also required after resynchronization. Pure opcode recognition does not prove alignment. Short but plausible false candidates can temporarily consume bytes; the timeout bounds waiting for an incomplete one. A total-frame timeout can additionally bound a sender that keeps a candidate alive; whether it is needed is open.

Timeouts are configurable inputs to the parser, not PC sleeps or an embedded timer dependency. Their eventual values must account for byte time, allowable sender gaps and USB/UART buffering. A short PCB connection does not eliminate the need to test resets, truncation and UART overrun/framing errors. No measured reliability claim is made by this draft.

## Implementation order and PC tests

Proceed in small steps after agreeing the remaining constants and codec interfaces:

1. Define frame types, opcode values, maximum sizes, register descriptors and error codes in a hardware-independent no_std Rust component.
2. Write complete host tests and a matching interface/stubs before filling in the implementation. Use the existing CRC abstraction rather than coupling the codec to a Pico engine.
3. Implement encoding/decoding and the incremental byte-stream parser; no flash, GPIO or real UART calls.
4. Separately test register access validation and transactional dispatch against fake state/storage/output.
5. Only then design the program loop and the UART adapter, including bounded servicing, polling/completion and flash-write flow control.

Host-test cases should cover exact wire vectors and sizes; READ request/response distinction; little-endian signed angles; complete calibration payloads; legal/illegal scalar and array lengths; unknown opcodes/indices; range overflow; invalid access; reserved control bits; CRC corruption in header/data/CRC; split frames and back-to-back frames; opcode-like payload bytes; noise prefixes and overlapping candidates; corrupt plausible lengths; truncation and injected timeout; bounded memory and resynchronization to the next valid request; and no dispatch or partial state mutation before validation. Include normal and canonical LastError responses and ensure success does not silently clear a previous error.

PC tests do not demonstrate physical UART timing, EMI tolerance, flash-operation flow control or hardware-safe stop behavior. Those require later integration and hardware checks.

## Open decisions before freezing version 1

- Numeric error codes; final concrete register map and protocol-version encoding.
- Current tests fix a four-byte header, 512-byte payload cap and 520-byte maximum frame; reconsider only if a later supported configuration requires it.
- Exact read/write permissions and status/control completion behavior in each state.
- Register-service error responses and transport-error reporting; framing tests cover the CRC-valid rejected-READ path, but no replies or register mutations are implemented yet.
- Calibration completion/restart/polling and master silence during blocking flash operations.
- Timeout values, event semantics and handling of delayed READ responses without request IDs.

This draft supersedes the earlier proposals for mandatory board IDs, a separate calibration-only service bus, per-command acknowledgements, partial protocol calibration, COBS/start-magic framing and a length expressed as register count. It does not alter the existing journal format, normal firmware, hardware tests or initialization code.

## Implemented framing layer and verification

The [servo-protocol package](../firmware/servo-protocol/README.md) is no_std and has no production dependency on UART, Pico or storage. The CRC function is supplied as `FnMut(&[u8]) -> u32`, with independent CRC-32/ISO-HDLC calculations required per call. Host tests reuse SoftwareCrc32 through a development-only servo-storage dependency. Fixed wire vectors were independently calculated using Python zlib.crc32.

Header handles four-byte encoding/decoding. RegisterRegion describes logical entries and fixed byte widths; Access selects permitted operations. validate_header checks the map, index, access, length and region boundary, independently of payload/state. FrameKind distinguishes Request and Response; encode_frame/decode_frame handle exact complete frames and reject trailing bytes. The generic frame decoder enforces its declared-length cap, including READ requests; the stream parser separately handles CRC-valid READ requests with invalid requested lengths so the register service can reply with LastError.

StreamParser owns a fixed 520-byte buffer. push accepts one byte or returns BufferFull without overwriting existing data. next_frame retires a previously returned frame, searches candidates and returns ParsedFrame with borrowed payload and its header-validation result. Borrowing prevents simultaneous mutation while that payload is used. Rejected candidates advance one byte so overlapping starts remain discoverable. Valid-frame bytes are removed only on the next mutable parsing/push call. Drain next_frame until None before delivering on_timeout; timeout clears the logical remainder without requiring byte zeroing. Register descriptors are checked at construction and reused for plausibility checks.

On 2026-10-31 the owner reported 50 passing host tests: 8 header, 12 validation, 12 complete-frame and 18 stream-parser cases, after observing their intended failing phases. Tests cover the stated framing paths, not register execution, actual error replies, semantic control bits, calibrated target dispatch or physical UART timing. The package is now registered in dev.py test/fmt/check/verify/doc; the owner subsequently reported 326 passing tests and all 15 verification checks passing, plus all 8 combined documentation checks passing. No UART adapter, firmware main-loop change or hardware protocol test was performed. Local checks are complete; the owner performs staging and committing. CI for this protocol revision is pending.
