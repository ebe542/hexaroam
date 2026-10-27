# Calibration data format

`hexaroam-servo-storage` is a hardware-independent, `no_std` library for encoding and decoding servo calibration records. It reuses `Calibration` from `hexaroam-servo-control`; it does not access flash, UART, DMA or servo outputs.

## Version 1 representation

All multibyte values are little endian. The record has no implicit padding.

| Field | Size | Representation |
| --- | --- | --- |
| Identifier and version | 2 bytes | `48 01` hexadecimal |
| Payload length | 2 bytes | `u16`, bytes occupied by entries |
| Entries | 20 bytes each | Servo ID `u32`, lower angle `i32`, lower pulse `u32`, upper angle `i32`, upper pulse `u32` |
| CRC | 4 bytes | CRC-32/ISO-HDLC over header and payload |

Angles are hundredths of a degree; pulse widths are microseconds. Eighteen entries occupy 368 bytes including header and CRC. An empty record occupies eight bytes. Flash-slot padding is outside the format and must not be passed to `decode()`.

## API and validation

`encode()` accepts an array of optional calibrations and an output byte slice. Array indices are Servo IDs. It writes present entries in ascending ID order and returns the encoded length; bytes beyond that length remain unchanged. Payloads exceeding the 16-bit length field and insufficient output space are rejected.

`decode()` accepts one exact record and returns an array of optional calibrations. Input entry order is arbitrary. It checks identifier/version, exact length, entry-size divisibility, CRC, unknown and duplicate IDs, and each entry through `Calibration::new()`. No partial result is returned on failure.

`None` means an absent or invalidated calibration. A partial or empty configuration is a valid storage record, but does not authorize robot operation. The later startup layer must require every configured servo to have a valid calibration. Format validation does not establish physical joint limits or compatibility with a particular signal backend.

## Checksum interface

`Checksum` allows interchangeable implementations. Each call starts a complete independent CRC-32/ISO-HDLC calculation: polynomial `0x04C11DB7`, initial value `0xFFFFFFFF`, reflected input/output, final XOR `0xFFFFFFFF`. The stored result is little endian. `SoftwareCrc32` is provided; a Pico hardware adapter is not yet implemented.

## Verification and remaining work

The owner reported 23 passing host tests: five CRC/adapter tests and eighteen format tests. Fixtures are constructed independently of the encoder. No embedded storage operations have been implemented or tested.

From the repository root:

```bash
cargo fmt --manifest-path firmware/servo-storage/Cargo.toml
cargo test --manifest-path firmware/servo-storage/Cargo.toml --lib --locked
```

Next steps are the append-only journal, interrupted-write behavior and a separate Pico flash driver. Two 4-KiB sectors and 512-byte slots are planned; no flash region is reserved yet. Service access, persistent invalidation and boot-mode enforcement are not implemented by this library.
