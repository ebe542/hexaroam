# Servo protocol

Hardware-independent no_std framing for the [register-protocol draft](../../docs/servo-register-protocol.md). This is the protocol layer between a future byte transport and register service. It does not execute commands, access registers/flash or perform UART I/O.

| Module | Responsibility |
| --- | --- |
| [header.rs](src/header.rs) | READ=1, WRITE=2, SET=3, CLR=4; u8 index and u16 little-endian byte length |
| [validation.rs](src/validation.rs) | RegisterRegion map, access/length checks and range boundaries |
| [frame.rs](src/frame.rs) | Exact complete-frame encoding/decoding and CRC verification; Request/Response distinction |
| [stream.rs](src/stream.rs) | Fixed-buffer incremental parser, overlapping-candidate recovery and explicit timeout reset |

The four-byte header is followed by at most 512 payload bytes and a four-byte CRC-32/ISO-HDLC. A READ request has no payload; its byte length describes the requested response. Only READ responses exist. Checksum calculation is supplied by the caller; production code has no dependency on the storage package. Tests alone use its SoftwareCrc32 implementation.

StreamParser accepts bytes through push. Repeatedly call next_frame with a checksum function to drain CRC-valid frames; ParsedFrame includes the header's validation result. Invalid READ requests can be delivered for a later LastError reply, while impossible non-READ candidates are skipped early. Payloads borrow parser storage and cannot outlive a subsequent mutable parser use. Drain available frames before on_timeout clears the incomplete remainder. BufferFull means the incoming byte was not accepted: the adapter must handle that event explicitly, not silently lose the byte and continue as if successful.

From the repository root:

```bash
cargo test --manifest-path firmware/servo-protocol/Cargo.toml --lib --locked
python tools/dev.py verify
python tools/dev.py doc
```

The owner reported 50 passing PC tests on 2026-10-31 after the intended red phases. Register-state/value validation, LastError reply generation, UART integration and actual link behavior remain future work. The owner subsequently reported 326 passing tests and all 15 verification checks passing, plus all 8 combined documentation checks passing. CI for this protocol revision is pending. No protocol code is wired into normal Pico firmware.
