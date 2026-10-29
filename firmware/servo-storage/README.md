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

## Journal slot representation

`encode_slot()` and `decode_slot()` wrap the existing calibration record in a fixed 512-byte slot. All multibyte fields remain little endian and are serialized explicitly, without Rust structure padding.

| Offset | Field | Size |
| --- | --- | --- |
| 0 | Journal identifier/version: `4A 01` hexadecimal | 2 bytes |
| 2 | Length of embedded calibration record, including its header and CRC | 2 bytes, `u16` |
| 4 | Sequence number | 8 bytes, `u64` |
| 12 | Embedded calibration record | Variable |
| 12 + embedded length | Journal CRC over journal header and embedded record | 4 bytes |
| After journal CRC | Padding | Remaining bytes |

Eighteen calibrations occupy 384 bytes including both envelopes and CRCs. The maximum embedded record length is 496 bytes. The outer CRC protects the sequence number as well as the complete inner record; the inner format remains independently usable.

`encode_slot()` builds a local candidate, fills unused bytes with `0xFF`, and leaves the caller's buffer unchanged on error. `decode_slot()` reports a completely erased slot as `SlotError::Empty`. Other slots must pass identifier, length, outer CRC and inner-record validation. Padding is ignored when decoding; that does not make a partially programmed slot free for reuse.

`JournalRecord` returns the sequence and optional calibrations. An empty or partial calibration record remains valid journal content, so a later reader must not resurrect an older complete configuration after intentional invalidation. The slot codec can represent `u64::MAX`; the journal writer rejects sequence wraparound.

These functions encode and validate bytes only. They neither program flash pages nor establish an atomic write-completion mechanism. Newest-record selection and duplicate-sequence detection are implemented below. Slot selection and sector switching are implemented by the storage-independent writer below; actual flash operations remain future work.

## Journal search

`find_latest()` accepts a slice of fixed-size slot images and returns `Result<Option<LocatedRecord<N>>, ScanError>`. It examines all supplied slots and selects the valid record with the highest sequence number, regardless of physical address or sector. `LocatedRecord` includes the slot index and decoded record. Slot images are read-only; no storage driver is involved.

Erased and invalid slots are skipped. No valid record returns `Ok(None)`, which must not authorize normal operation. A found record returns `Ok(Some(...))`; the later startup layer still needs to require every configured calibration to be present. A newer empty or partial configuration remains authoritative and must not be replaced by an older complete one.

Any duplicate sequence number among valid records returns `ScanError::DuplicateSequence`, including duplicates older than the newest record. Invalid records do not participate in duplicate detection. `u64::MAX` can be selected without incrementing it; the writer rejects further appends after that sequence.

The current search re-decodes previous slots for duplicate detection rather than allocating a sequence-number table. Its work is quadratic in the slot count; the planned journal has only sixteen slots. CRC adapters must start an independent computation on every call.

Host tests cover both sector/address orders, missing and invalid records, duplicate sequences, partial/empty invalidation, corrupted sequence numbers, invalid inner records, maximum sequence and a simulated incomplete header write. Write-management tests below simulate selected interrupted operations. Actual interrupted flash-program/erase recovery still requires hardware checks.

## Journal storage and append

`JournalRead` provides complete-slot reading. `JournalStorage: JournalRead` adds programming an erased slot and erasing one sector; read-only backends need not expose writes. Geometry is two sectors with eight 512-byte slots each. Backend errors may leave a slot partially programmed or a sector partially erased. The real backend is responsible for the platform execution conditions; this trait alone does not make flash writes safe.

`append()` reads all sixteen slots before selecting a destination. A read error aborts the operation; it is not treated as an invalid record. The newest valid record determines the active sector and next sequence. With no valid record, initialization uses sector0 and sequence1; no claim of globally unique numbering after complete data loss is made. Duplicate valid sequences and sequence overflow prevent writes. The candidate is encoded before any erase/program call, so an oversized record cannot trigger a sector erase.

Within the active sector, the writer chooses the slot after the last occupied slot. Any byte other than `0xFF`, including padding-only changes, marks a slot occupied. Erased holes before the last occupied slot are not reused. If the active sector is full, the writer switches to the other sector and erases it only if it is not completely erased. The sector containing the latest valid record is retained; it is only reclaimed when a later switch needs it.

After programming, the writer reads back and compares all 512 bytes, including padding, against the encoded candidate. Only an exact match returns `LocatedRecord`. Read/write/erase errors are returned as `JournalError::Storage`; a mismatch returns `VerificationFailed`. Every retry starts from a fresh storage scan, so a partial failed write is not blindly overwritten. An error is not a rollback guarantee: a complete record may already be present if writing succeeded but readback failed. Future UART/service callers must treat that outcome as uncertain and reload before assuming which state is durable.

The implementation currently holds an 8-KiB journal image plus two 512-byte buffers in `append()`. Nested encoding and decoding use additional stack space; this is not a measured total stack bound. Embedded stack use must be reviewed before Pico integration.

The sixteen writer tests cover first append, sequence progression, occupied-slot skipping, active-sector selection, partial invalidation, both sector-switch directions, avoiding unnecessary erases, duplicate/overflow rejection, scan/readback errors, a partial write and retry, a partial erase failure, full-buffer readback mismatch including padding, oversized records and the final available sequence. These are simulations, not an exhaustive power-loss campaign or hardware verification. Startup/service-mode enforcement and Pico multicore/interrupt/DMA coordination are not implemented here. The Pico linker reservation is documented below.

## Verification and remaining work

On 2026-10-28 the owner reported 66 passing storage tests: five CRC/adapter tests, eighteen calibration-format tests, thirteen journal-slot tests, fourteen journal-search tests and sixteen writer tests. The full local runner subsequently reported 238 passing tests and all 13 checks passing. Envelope fixtures are constructed independently of the corresponding envelope encoder. The owner subsequently reported green CI for the combined search/write-management revision. These results describe the journal-management revision. The subsequent read-only Pico integration is recorded below; embedded writes and erases remain unimplemented.

From the repository root:

```bash
cargo fmt --manifest-path firmware/servo-storage/Cargo.toml
cargo test --manifest-path firmware/servo-storage/Cargo.toml --lib --locked
```

Next steps are additional recovery scenarios as needed and a separate Pico flash driver. The Pico linker now reserves two 4-KiB sectors with eight 512-byte slots each; a read-only Pico backend is available; flash programming and erase are not implemented. Service access, persistent invalidation and boot-mode enforcement are not implemented by this library.

## Pico flash reservation

The Pico [memory.x](../pico/memory.x) reserves the final 8 KiB of its 2-MiB flash for the journal. Firmware FLASH length excludes that region; the existing 256-byte BOOT2 area is unchanged.

| Region | Memory-mapped address | Flash-relative offset | Size |
| --- | --- | --- | --- |
| Journal sector0 | `0x101FE000` | `0x001FE000` | 4096 bytes |
| Journal sector1 | `0x101FF000` | `0x001FF000` | 4096 bytes |
| Journal end, exclusive | `0x10200000` | `0x00200000` | - |

Linker symbols `__calibration_start` and `__calibration_end` expose these boundaries. Assertions check adjacency to firmware, 4-KiB start alignment, two-sector size and the end of flash. No initialized data section is assigned to CALIBRATION. This reserves space against firmware growth at link time; it does not prevent a full-chip erase, programming other firmware, or explicit writes to those addresses.

On 2026-10-28 the owner reported 238 passing tests and all 13 checks passing after the linker change, including the Pico build. Read-only inspection of that debug ELF confirmed both boundary symbols and no load-image or runtime segment overlapping the journal. The highest flash load-image end was `0x10015C50` (exclusive); this is a build-specific observation, not a fixed firmware size. No UF2 upload, persistence check or flash operation was performed for this reservation. The owner subsequently reported green CI for the reservation change.

## Read-only loading and Pico integration

`load()` requires only `JournalRead`, reads all sixteen slots and calls `find_latest()`. A storage read error aborts loading instead of returning an earlier record. Six host tests cover a read-only backend, absent records, latest-record selection, intentional invalidation, corrupt records, duplicate sequences and a read failure after an earlier valid record.

The Pico [FlashJournalReader](../pico/src/flash_journal.rs) obtains addresses from the linker symbols, checks region size/alignment and flash bounds, rejects invalid slot indices, and copies each slot into caller-owned RAM. It implements `JournalRead` only. Its unsafe constructor requires normal readable 2-MiB XIP flash and no flash programming/erase or XIP reconfiguration by any core or handler for the reader's entire lifetime. Address checks do not establish synchronization; a future writer must account for the reader's lifetime.

The standalone [flash_journal_read hardware test](../pico/hardware_tests/flash_journal_read.rs) loads for eighteen configured servos using SoftwareCrc32, checks rejection of index SLOT_COUNT, and reports one result through UART1. It starts no servo output and performs no flash programming or erase. The normal fixed-plan demo is unchanged.

On 2026-10-28 the owner reported 72 passing storage tests, successful hardware-test execution with `INFO Journal read OK: no valid record`, and a full local verification of 244 passing tests and all 13 checks passing. The message confirms completed reading/search with no accepted record; it does not prove that all bytes are erased. LED state was not separately reported. The owner subsequently reported green CI for the read-only integration. Loading a known valid flash record, physical read-error handling, persistence, writes and erases have not been hardware-tested.

## Preparation for embedded writes

The separate [RAM-execution hardware probe](../pico/hardware_tests/README.md#ram-routine-probe) was verified on 2026-10-29 by ELF inspection and the owner-observed `INFO RAM routine OK`. The full local runner remained at 244 passing tests and 13/13 checks passing. This preparation does not implement JournalStorage on the Pico: XIP remained enabled, and no flash was erased or programmed. The subsequent standalone no-write XIP transition is recorded below. Embedded storage writes, general runtime exclusion and persistence tests remain to be implemented and checked.

The [XIP transition test](../pico/hardware_tests/README.md#xip-exit-and-restoration-test) subsequently passed on 2026-10-29 with owner-observed `INFO XIP cycle OK`, 244 passing tests and 13/13 local checks. The test prepares all ROM pointers and its Boot2 RAM copy before leaving XIP, restores XIP before returning, and checks a mapped flash word afterward. No flash programming/erase occurred and JournalStorage is still not implemented on the Pico. Multicore/DMA exclusion is a standalone-test condition, not an implemented service-mode policy. CI for this revision is pending.
