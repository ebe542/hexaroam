# Servo protocol

Hardware-independent no_std framing for the [register-protocol draft](../../docs/servo-register-protocol.md). This is the protocol layer between a byte transport and register service. It does not execute commands, access registers/flash or perform UART I/O.

| Module | Responsibility |
| --- | --- |
| [header.rs](src/header.rs) | READ=1, WRITE=2, SET=3, CLR=4, ACK=0x80, NACK=0x81; u8 index and u16 little-endian byte length |
| [validation.rs](src/validation.rs) | RegisterRegion map, access/length checks and range boundaries |
| [frame.rs](src/frame.rs) | Exact complete-frame encoding/decoding and CRC verification; Request/Response distinction |
| [stream.rs](src/stream.rs) | Fixed-buffer incremental parser, overlapping-candidate recovery and explicit timeout reset |
| [transport.rs](src/transport.rs) | Nonblocking byte receive/transmit contract |
| [response_buffer.rs](src/response_buffer.rs) | One owned response and bounded transmission progress |

The four-byte header is followed by at most 512 payload bytes and a four-byte CRC-32/ISO-HDLC. READ requests have no payload; their length specifies requested response data. Responses use ACK=0x80 (zero or more data bytes) or NACK=0x81 (exactly one error byte). Request/response opcode directions are enforced. Checksum calculation is supplied by the caller; production code has no storage dependency. Host tests use SoftwareCrc32.

## Stream parsing

StreamParser checks known request indices and plausible lengths before receiving payload/CRC, independently of operation permissions. Unknown indices, invalid lengths and crossing ranges are discarded without a response. Plausible prohibited operations are CRC-checked and delivered with their access error for a later NACK. Response parsing does not apply register access rules; an empty ACK or a NACK referring to an unknown index is valid.

Rejected candidates advance one byte to preserve overlapping starts. Plausibility filtering improves recovery but cannot guarantee immediate resynchronization after arbitrary noise. A false candidate with plausible fields can require the receive timeout. Payloads borrow parser storage; drain next_frame before on_timeout clears an incomplete remainder. BufferFull rejects the new byte without overwrite, requiring explicit caller handling.

## Nonblocking byte transport and response buffer

ByteTransport defines try_read returning a byte or None and try_write returning true only after acceptance. False or Err means the byte was not accepted. These operations do not wait for UART capacity; transport acceptance does not prove physical wire completion. This is separate from diagnostic logging.

ResponseBuffer<CAPACITY> copies one complete encoded response and tracks its unsent suffix. queue rejects pending work with Busy before Empty/TooLarge; rejected operations preserve data and progress. service uses a byte budget, stops on backpressure and retains the failed byte on transport errors. Retrying is an explicit caller decision. Empty service and zero budgets do not access the transport.

[FrameProcessor](../servo-registers/README.md#frame-processing-over-a-byte-transport) coordinates this buffer, the parser and register service. The concrete [UartTransport](../apps/servo-controller/src/uart_transport.rs) uses the HAL UART; its loopback and no-servo protocol test are separate hardware examples. Normal Pico firmware is not yet the protocol-driven servo application.

## Verification

From the repository root:

```bash
cargo test --manifest-path firmware/servo-protocol/Cargo.toml --lib --locked
python tools/dev.py verify
python tools/dev.py doc
```

On 2026-11-03 the owner reported all **82 protocol tests** passing and full local verification of **462 tests and 17 checks** passing. The version-2 no-servo UART suite separately passed all 21 scenarios on hardware. CI for this revision has not yet been reported. See the [protocol contract](../../docs/servo-register-protocol.md) and [UART vectors](../../docs/uart-register-test-vectors.md) for scope and limitations.
