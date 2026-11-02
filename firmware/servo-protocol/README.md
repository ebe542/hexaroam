# Servo protocol

Hardware-independent no_std framing for the [register-protocol draft](../../docs/servo-register-protocol.md). This is the protocol layer between a future byte transport and register service. It does not execute commands, access registers/flash or perform UART I/O.

| Module | Responsibility |
| --- | --- |
| [header.rs](src/header.rs) | READ=1, WRITE=2, SET=3, CLR=4; u8 index and u16 little-endian byte length |
| [validation.rs](src/validation.rs) | RegisterRegion map, access/length checks and range boundaries |
| [frame.rs](src/frame.rs) | Exact complete-frame encoding/decoding and CRC verification; Request/Response distinction |
| [stream.rs](src/stream.rs) | Fixed-buffer incremental parser, overlapping-candidate recovery and explicit timeout reset |
| [transport.rs](src/transport.rs) | Nonblocking byte receive/transmit contract |
| [response_buffer.rs](src/response_buffer.rs) | One owned response and bounded transmission progress |

The four-byte header is followed by at most 512 payload bytes and a four-byte CRC-32/ISO-HDLC. A READ request has no payload; its byte length describes the requested response. Only READ responses exist. Checksum calculation is supplied by the caller; production code has no dependency on the storage package. Tests alone use its SoftwareCrc32 implementation.

StreamParser accepts bytes through push. Repeatedly call next_frame with a checksum function to drain CRC-valid frames; ParsedFrame includes the header's validation result. Invalid READ requests can be delivered for a later LastError reply, while impossible non-READ candidates are skipped early. Payloads borrow parser storage and cannot outlive a subsequent mutable parser use. Drain available frames before on_timeout clears the incomplete remainder. BufferFull means the incoming byte was not accepted: the adapter must handle that event explicitly, not silently lose the byte and continue as if successful.

From the repository root:

```bash
cargo test --manifest-path firmware/servo-protocol/Cargo.toml --lib --locked
python tools/dev.py verify
python tools/dev.py doc
```

The owner reported 50 passing PC tests on 2026-10-31 after the intended red phases. Register-state/value validation, LastError reply generation, UART integration and actual link behavior remain future work. The owner subsequently reported 326 passing tests and all 15 verification checks passing, plus all 8 combined documentation checks passing. CI for this protocol revision is pending. No protocol code is wired into normal Pico firmware.

## Nonblocking byte transport and response buffer

ByteTransport defines try_read returning one byte or None, and try_write returning true only when its byte was accepted. False means temporary backpressure without acceptance; Err also means the byte was not accepted. Methods must not wait for input or transmit capacity. Acceptance is not physical wire completion. This trait has no UART dependency and is separate from the logging interface.

ResponseBuffer<CAPACITY> owns one copied encoded response, its length and the next unsent offset. The intended full-frame capacity is MAX_FRAME_SIZE (520 bytes). queue rejects pending work with Busy before checking Empty or TooLarge; every rejection preserves data and progress. It neither encodes nor validates a frame or CRC. A second response cannot overwrite the first. The caller may reuse its source bytes after queue returns.

service sends at most its byte budget, stops on backpressure and advances only accepted bytes. A transport error returns immediately, preserving the failed byte and unsent suffix; any earlier progress remains committed. Retrying is an explicit caller decision. Completion makes the buffer reusable, including for a shorter response. Empty service and a zero budget do not access the transport. This bounds transport calls, not elapsed execution time independently of the backend contract.

Twelve tests in [test_response_buffer.rs](src/test_response_buffer.rs) cover empty/busy/oversized requests, budgeted order, backpressure before and after progress, write errors and explicit retry, zero budget, reuse, copied input and zero-capacity behavior. On 2026-11-02 the owner observed the intended red phase (50 passed, 12 failed), then confirmed all 62 package tests passing. Full verification reported 439 tests and all 17 checks passing. No tests were executed by the assistant. CI for this revision is pending.

The Pico now provides a concrete UartTransport implementation in addition to the test transports; its UART0 loopback test is described below. Receiving through a real UART, timeout timing and the firmware loop remain integration work. Parser/service scheduling and response-buffer coordination are now implemented by FrameProcessor in servo-registers. The response-buffer tests exercise transmission, not try_read behavior. The normal Pico firmware is unchanged.

The owner subsequently included the [FrameProcessor integration](../servo-registers/README.md#frame-processing-over-a-byte-transport) in the same revision. It owns the parser, register service and response buffer, preserves complete requests across deferred receive timeouts and continues output servicing during response backpressure. Twelve additional register-package tests passed after their red phase; final full verification reported 451 tests and all 17 checks passing on 2026-11-02. The protocol package remains at 62 tests. CI for this combined revision is pending; a concrete UART transport is not yet implemented.

## Concrete Pico UART transport

[UartTransport](../pico/src/uart_transport.rs) implements ByteTransport over an enabled HAL UART, preserving nonblocking empty/full behavior and reporting receive errors explicitly. The owner confirmed the [UART0 loopback](../pico/hardware_tests/README.md#protocol-uart-byte-transport-loopback) on 2026-11-02: all 1024 test bytes matched, with separate debug UART output. Full verification reported 451 tests and all 17 checks passing. The normal firmware and FrameProcessor-to-UART path remain unchanged; physical protocol exchange, timeout timing and error injection are separate work. CI for this UART-driver revision is pending.
