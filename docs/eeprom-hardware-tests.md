# Manual EEPROM write/read and write-protection tests

These separately selected Pico firmware examples exercise the actual 24xx256 byte driver over I2C1. They do not start servo signals, DMA, the command protocol or calibration storage management. Neither test has been run on a physical EEPROM yet.

## Test data and scope

Use a test EEPROM without required calibration data. eeprom_write_read overwrites the final 128 bytes, 0x7F80 through 0x7FFF, on every boot. This is a test-only area, not a permanently reserved region in the production 64-slot calibration layout; it lies inside production slot 63.

eeprom_write_protect attempts to overwrite the same region. If WP is ineffective, data can change and is not automatically restored. Run it after a successful write/read test on the same EEPROM. Unchanged contents alone do not identify WP as the cause; this prior positive write check matters.

There is no separate read-only or power-cycle persistence example. The initial eeprom_read target was removed in favor of these two tests. Successful immediate readback does not establish retention across power loss. Stale eeprom_read build files may remain in ignored target directories; they are not registered test targets.

## Wiring

Power the Pico through USB and the EEPROM from Pico 3V3. Use a separate test setup with configurable WP. The revision D operating-controller schematic ties WP high; do not short that fixed 3V3 connection to GND to run a write test.

| EEPROM DIP-8 pin | Connection |
| --- | --- |
| 1, 2, 3: A0, A1, A2 | GND, selecting seven-bit address 0x50 |
| 4: GND | Pico GND, for example physical pin 23 |
| 5: SDA | GP18, Pico physical pin 24; 2k pull-up to 3V3 |
| 6: SCL | GP19, Pico physical pin 25; separate 2k pull-up to 3V3 |
| 7: WP | GND for write/read; 3V3 for write protection |
| 8: VCC | Pico 3V3, physical pin 36 |

Place a 100nF capacitor directly between EEPROM pins 8 and 4. The firmware configures I2C1 at 400kHz and disables Pico internal pull resistors; the external resistors define both lines. Signal quality and timing still require measurement on the assembled circuit. See the archived [datasheet](../hardware/references/eeprom/24AA256-MIC.pdf) and [revision D pin allocation](../hardware/electronics/pico-servo-system/README.md#eeprom-and-peripheral-allocation).

A three-pin test jumper may use GND - WP - 3V3, with WP in the middle. Keep WP at a defined level and move the jumper only with power disconnected. The later operating design keeps WP permanently high.

For debug output connect GP20, Pico physical pin 26, to the adapter TTL RXD and connect common GND. Use the Waveshare adapter in 3.3V TTL mode, leaving its TXD and power outputs disconnected. Open HTerm on the owner's current COM6 at 115200 baud, 8N1, no flow control. Lines end in LF only. The Pico's USB connection supplies power/programming, not the log terminal.

## Build

From the repository root:

```bash
python tools/dev.py fmt
python tools/dev.py hardware-build
python tools/dev.py verify
```

hardware-build creates ELF, UF2, map and disassembly files in firmware/pico/target/thumbv6m-none-eabi/debug/examples. It does not upload or execute firmware. Use only artifacts from a fully successful build.

| Target | Firmware |
| --- | --- |
| eeprom_write_read | eeprom_write_read.uf2 |
| eeprom_write_protect | eeprom_write_protect.uf2 |

Both are Cargo examples behind hardware-tests. The shared Board initializer exposes I2C1; firmware/pico depends on hexaroam-eeprom. The normal Pico program remains unchanged. Cargo.lock records the added dependency.

## Write/read procedure

1. Disconnect power and set WP low on the test setup.
2. Transfer eeprom_write_read.uf2 using BOOTSEL. The test starts when firmware boots; no terminal command arms it.
3. Observe the debug output and LED.

The test first reads a 64-byte guard region at 0x7F40 through 0x7F7F. It writes 0xA5 to all 128 test bytes and verifies them, then writes eight patch bytes at relative offset 61 (absolute 0x7FBD): 11 22 33 44 55 66 77 88 hexadecimal. This crosses a physical page boundary, requiring writes of three and five bytes. It then compares the complete 128-byte region, including unchanged bytes around the patch, and checks that the preceding guard still matches its original contents.

Expected output:

```text
INFO EEPROM write/read started: 0x7F80..0x7FFF
INFO EEPROM write/read OK
```

On success the LED remains on. A reported test error leaves it off and emits ERROR with the failing phase. The test runs once per boot and then only services logging; restarting repeats the destructive test. It verifies immediate readback, page splitting and the preceding guard, not power-cycle retention or calibration parsing.

## Write-protection procedure

1. Complete the positive write/read test successfully on the same EEPROM.
2. Disconnect power and set WP high.
3. Transfer eeprom_write_protect.uf2 and observe output.

This test reads the existing 128 bytes, attempts to write the bitwise inverse of every byte, then reads the region again. Every attempted bit differs from the snapshot, so identical before/after contents cannot be explained by writing the same data. A write-transaction error is a failure, not a successful protection result. The EEPROM is expected to acknowledge protected writes while leaving memory unchanged.

Expected output:

```text
INFO EEPROM WP test started: WP must be high
INFO EEPROM write protection OK
```

On success the LED remains on. If contents change, the result is ERROR EEPROM WP failed: contents changed. A failed test does not restore the snapshot. Both examples use the UART panic handler for unexpected assertions/expect failures; a PANIC is not a passing result.

## Verification record and limits

On 2026-11-08 the owner reported all 63 hardware-build checks passing after replacing the standalone reader with these two examples. Full local verification passed 837 tests and all 21 checks. These results establish compilation/artifact generation and the existing PC tests, not hardware execution. The assistant did not run builds, tests or upload firmware.

Pending: physical write/read and WP runs, I2C waveforms/rise times, EEPROM retention/power-loss checks, and calibration-storage/boot integration on hardware. The synchronous HAL I2C operations have no application-level timeout or watchdog recovery in these examples; bounded EEPROM readiness retries do not bound a stalled underlying bus call. The start log is submitted to UART before blocking I2C access.
