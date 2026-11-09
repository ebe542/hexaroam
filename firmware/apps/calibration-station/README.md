# Calibration station

Independent Raspberry Pi Pico (RP2040) application for the single-servo calibration bench. It will drive raw pulse widths for measuring angle-to-pulse relationships and later program the external EEPROM. The robot controller consumes calibration data separately.

## Current implementation

The firmware initializes clocks, the onboard LED, a hardware timer and debug UART1. The LED toggles every 500 ms. One startup line is queued through the shared nonblocking logger:

```text
INFO Calibration station ready
```

The main loop services the logger continuously without periodic log messages. Panic output uses the copied bounded UART1 handler. The UART driver and panic handler are currently local copies from the controller application.

Servo output, PC protocol commands and EEPROM access are not implemented yet. No servo signal is generated; this revision is a startup check rather than a calibration procedure.

## Build and upload

Run from the repository root:

```bash
python tools/dev.py build --app calibration-station
python tools/dev.py uf2 --app calibration-station
```

The first command builds ELF; the second builds and converts through the Cargo runner. Neither uploads firmware. Copy `firmware/apps/calibration-station/target/thumbv6m-none-eabi/debug/calibration-station.uf2` to the Pico's BOOTSEL drive manually.

The local `.cargo/config.toml` selects `thumbv6m-none-eabi`. `build.rs` supplies `memory.x`, which includes the BOOT2 section and uses the Pico flash without the controller's historical journal reservation. Commit the application's Cargo.lock.

## Startup hardware check

Use USB to power and program the Pico. Connect GP20 (physical pin 26), UART1 TX, to a 3.3 V TTL adapter RXD and connect common GND. Leave the adapter TXD and power outputs disconnected. Select TTL mode rather than RS232 or RS485. UART settings are 115200 baud, 8 data bits, no parity, one stop bit and no flow control; log lines end in LF. Open the terminal before resetting to capture the single startup message.

Expected behavior: the onboard LED blinks and the startup message appears. No servo or EEPROM connection is required for this check. USB serial logging is not implemented.

The owner confirmed upload, LED blinking and UART1 output. This does not validate servo wiring, servo motion or EEPROM operation.

## Shared checks and hardware tests

```bash
python tools/dev.py verify
python tools/dev.py hardware-build --app calibration-station
```

`verify` includes both applications and the shared library/tool tests. No station hardware-test sources exist yet, so hardware-build reports `[SKIP] calibration-station: no hardware-test sources` immediately and in its Summary. This is not a hardware test execution.

The owner reported 854 tests, zero failures, all 23 verify checks and all 63 controller hardware-build checks passing. CI for this revision is pending.

See [development tools](../../../tools/README.md#application-selection) and the [firmware overview](../../README.md).
