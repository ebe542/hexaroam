# Calibration station

Independent Raspberry Pi Pico (RP2040) application for the single-servo calibration bench. It will drive raw pulse widths for measuring angle-to-pulse relationships and later program the external EEPROM. The robot controller consumes calibration data separately.

## Current implementation

The firmware initializes clocks, the onboard LED, a hardware timer and debug UART1. The LED toggles every 500 ms. One startup line is queued through the shared nonblocking logger:

```text
INFO Calibration station ready
```

The main loop services the logger continuously without periodic log messages. Panic output uses the copied bounded UART1 handler. The UART driver and panic handler are currently local copies from the controller application.

The main application does not generate servo signals yet; UART command integration and EEPROM access remain pending. A separate manual hardware test now exercises single-channel PIO/DMA output without angle calibration.

## Application-owned logic

Station-specific code lives in this application rather than shared firmware crates:

| File | Responsibility |
| --- | --- |
| `src/lib.rs` | Hardware-independent library entry point for host tests |
| `src/command.rs` and `src/test_command.rs` | Station request decoding and its 12 tests |
| `src/control.rs` and `src/test_control.rs` | Raw-pulse service, PulseBackend contract and its 14 tests |
| `src/command_handler.rs` and `src/test_command_handler.rs` | Request-to-service dispatch, ACK/NACK content and its 14 tests |
| `src/main.rs` | Pico startup and the hardware loop |

`firmware/servo-protocol` continues to provide shared COBS/CRC32 framing, byte transport and common protocol error types. It no longer contains the station command implementation. The separate calibration-control crate was folded into this application; controller-specific code is unchanged.

The request decoder accepts GET_STATUS, OUTPUT_ENABLE and SET_PULSE. The service validates pulse limits, prepares a pulse while inactive, controls startup/stop, preserves the last accepted value on Busy and latches output errors. The command handler connects the decoder to the service and returns fixed-capacity ACK/NACK content. These modules are not yet connected to UART requests or the PIO backend in the main application. See the [station protocol contract](protocol.md) for commands, response layouts, flags and error behavior.

Embedded dependencies are limited to ARM bare-metal targets. Host tests compile only the application library and its hardware-independent dependencies. The build script supplies the Pico linker layout only for bare-metal targets.

Run all shared tests through `python tools/dev.py test` or `python tools/dev.py verify`. Both include the station library with an explicit host target, overriding its local Pico default. For just these 40 Rust tests:

```bash
(cd firmware/apps/calibration-station && cargo test --lib --locked --target host-tuple)
```

`python tools/dev.py doc` documents the station library; the controller continues to document its binary. The existing manual hardware example is unchanged. The owner verified the restructuring and the subsequent request handler: full local verification passed 895 tests with zero failures and all 24 checks. CI for this command/control revision is pending.

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

`verify` includes both applications and the shared library/tool tests. `hardware-build --app calibration-station` builds the registered single_servo_output example and generates ELF, UF2, map and disassembly artifacts. It does not upload or execute the test. See the [manual hardware procedure](hardware_tests/README.md).

The owner reported 854 tests, zero failures and all 23 verify checks passing after adding the single-channel example, and confirmed the expected GP0 and UART1 behavior. CI was subsequently reported green for the single-channel hardware-test commit. That hardware observation does not validate the later command handler on UART; its checks are currently host tests only.

See [development tools](../../../tools/README.md#application-selection) and the [firmware overview](../../README.md).
