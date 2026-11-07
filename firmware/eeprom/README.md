# 24xx256 EEPROM driver

`hexaroam-eeprom` is a `no_std` byte-access driver for the 32 KiB Microchip 24AA256/24LC256/24FC256 family. It uses `embedded-hal` 1.0 `I2c` and, for writes, a caller-provided `DelayNs`. It contains no Pico GPIO configuration, calibration parsing, CRC, journal management or heap allocation in production code.

The selected project component is the 24LC256-I/P. The archived [family datasheet](../../hardware/references/eeprom/24AA256-MIC.pdf), revision DS21203P (2007), documents addressing and page writes in sections 5 and 6, ACK polling in section 7, and reads in section 8. See the [reference inventory](../../hardware/references/README.md) for provenance and checksum information.

## API

| Item | Contract |
| --- | --- |
| `CAPACITY` | 32,768 bytes; usable addresses 0x0000 through 0x7FFF |
| `PAGE_SIZE` | 64 bytes per physical write page |
| `Eeprom::new(i2c, address)` | Takes ownership of the bus; validates seven-bit device address 0x50 through 0x57 without bus access |
| `read(offset, output)` | Sends the two-byte memory address, high byte first, then reads using a combined write/read transaction with repeated START |
| `write(offset, data, delay)` | Splits data at physical page boundaries and waits for readiness after every chunk, including the last |

The device address depends on A0-A2; it is distinct from the two-byte memory address. Sequential reads may cross write-page boundaries. Reads and writes reject an out-of-range span before bus access rather than allowing the device's address wraparound. Empty operations at offsets 0 through 0x8000 succeed without bus access or delays; larger offsets are rejected even for an empty slice.

## Write completion and errors

After each page write, the driver immediately probes readiness by writing only that chunk's two memory-address bytes. No data bytes follow, so the probe sets the internal pointer without programming EEPROM contents. This avoids depending on HAL support for empty writes and differs from a pure address-only ACK poll.

An address NACK, or a NACK whose source cannot be determined, permits another probe after a 1 ms delay. At most ten delays and eleven probes are performed per chunk. This provides margin over the datasheet's 5 ms maximum internal write cycle. The limit bounds retry count and requested delays, not elapsed wall time: I2C transactions and scheduling take additional time, and a blocking HAL call needs its own timeout if required.

`EepromError` distinguishes invalid device addresses, out-of-bounds ranges, write-readiness timeout and the original bus error. A data NACK during readiness polling is not classified as busy. Page-write errors and other probe errors return immediately; failed page writes are not automatically retried. Earlier chunks or a failing transfer may already have modified storage. After a read bus error, the output buffer must not be interpreted.

A successful `write()` means transfers completed and the device became ready; it does not prove the data was stored. With WP high, the EEPROM can acknowledge writes without changing memory. Readback comparison belongs to the storage management layer.

## Calibration architecture

The operating servo controller will read calibration at startup only. EEPROM programming belongs to a separate PC-controlled calibration station using another Pico; it is not exposed by the robot's command protocol. The EEPROM is disconnected from the unpowered robot before station access. Hardware write protection in the robot is planned; the driver does not configure WP.

The separate [calibration storage contracts](../servo-storage/README.md#medium-independent-calibration-storage) expose complete datasets. The adapter connecting those contracts to this byte driver, the EEPROM journal layout, station firmware and boot integration are not implemented yet. Existing flash journal tests remain separate.

## Verification

From the repository root:

```bash
cargo test --manifest-path firmware/eeprom/Cargo.toml
python tools/dev.py fmt
python tools/dev.py verify
```

The owner reported all 32 EEPROM tests passing, followed by 697 tests and all 19 full-verification checks passing. The 13 constructor/read tests and 19 write-related tests use scripted I2C operations and delays on the PC. They cover address byte order, bounds, empty operations, page splitting, readiness retries, timeout boundaries and error propagation. They do not emulate EEPROM memory or verify electrical behavior.

The driver is registered in the development runner's test, fmt, check, verify and doc tasks; existing CI invokes those shared tasks. CI for this change has not yet been reported. No real EEPROM read/write, WP behavior or Pico I2C integration has been hardware-verified for this driver.
