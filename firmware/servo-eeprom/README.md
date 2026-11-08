# EEPROM calibration reader

`hexaroam-servo-eeprom` connects the medium-independent calibration storage API to the 24xx256 byte driver. This separate `no_std` crate keeps the [EEPROM driver](../eeprom/README.md) independent of servo data and [servo-storage](../servo-storage/README.md) independent of I2C devices.

## Ownership and interfaces

`EepromCalibrationReader<I, C, N, SLOTS = 64>` owns an `Eeprom<I>` and a checksum implementation `C`. It implements `CalibrationRead<N>` and returns a complete `[Calibration; N]`, None for no valid record, or an error. Array indices are Servo IDs.

Construction validates `SlotLayout::new(0, SLOTS, CAPACITY)` without bus access. A zero-slot or oversized region is rejected. The internal `EepromSlots` implements `JournalRead`: it maps a slot index to a byte address and delegates the complete 512-byte read to the EEPROM driver. Invalid indices are rejected before bus access. `load()` delegates to `load_calibrations`; it does not duplicate format or CRC logic.

## EEPROM layout

| Property | Value |
| --- | --- |
| Capacity | 32,768 bytes |
| Slot size | 512 bytes, eight 64-byte write pages |
| Default slot count | 64 |
| Slot address | index multiplied by 512 |
| Last slot | address 32,256 through 32,767 |
| Representation | Existing version-1 journal slot, unchanged |

The adapter can use a smaller nonempty region starting at zero. A slot holds up to 24 calibration entries in the existing format; all entries required by N must be present to load a complete dataset. Configuring N does not enlarge a slot.

The newest valid record is selected by its u64 sequence, not physical slot order. Each slot is read once in ascending order on a successful scan. RAM holds one slot buffer, the latest candidate and a table of sequence numbers, rather than the entire EEPROM image.

## Validation and errors

Format and CRC-invalid records are skipped. Any storage read error aborts loading, even if an earlier slot was valid: an unreadable slot might contain newer data. Duplicate sequences among valid records are errors, including identical records and duplicates older than the highest sequence.

Only after selecting the newest valid record does the loader require complete calibration. A missing entry returns `IncompleteCalibration` with its Servo ID; an intact but incomplete newest record must not fall back to an older complete record. An empty, valid record is therefore an error for N greater than zero, not None. Zero servos are rejected before bus access. Format checks do not prove mechanical mounting or application-specific pulse compatibility.

`EepromSlotError` preserves layout errors and underlying EEPROM errors. `CalibrationRead::Error` wraps these in `CalibrationLoadError`, preserving bus errors and distinguishing them from duplicate sequences and incomplete calibration.

## Verification and remaining work

From the repository root:

```bash
cargo test --manifest-path firmware/servo-eeprom/Cargo.toml
python tools/dev.py fmt
python tools/dev.py verify
```

The owner reported all 15 adapter tests passing, then 744 tests and all 21 full local checks passing. Adapter tests use the actual EEPROM driver, encoder and loader with a scripted I2C bus supplying prepared slot contents. They cover address mapping, all 64 slots, invalid regions/indices, record selection, corruption, completeness, duplicate sequences and bus error propagation.

The package and Cargo.lock are registered for the development runner's test, fmt, check, verify and doc tasks. Existing CI invokes the shared tasks; CI for this revision has not yet been reported.

No real EEPROM or Pico I2C hardware test has run for this adapter. Boot integration, the EEPROM writer/rotation and calibration-station application remain pending. The operating robot will only read calibration during startup; the station writes the EEPROM separately while it is disconnected from the unpowered robot. The existing flash journal and hardware tests remain separate.
