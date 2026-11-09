# Calibration-station hardware tests

Manual Pico firmware examples, registered in the station Cargo.toml behind `hardware-tests`. They are built separately and are not executed by host tests or CI.

## Single-channel output

Source: [single_servo_output.rs](single_servo_output.rs).

The test uses the existing PIO/DMA switching-output driver directly, without ServoBank, joint angles or calibration data. One PIO state machine drives GP0. Two static four-word buffers hold the single-channel plans: one High interval and one Low interval, each encoded as a GPIO mask and duration.

Build from the repository root:

```bash
python tools/dev.py hardware-build --app calibration-station
```

The task builds the example and generates its linker map, disassembly and UF2. Copy this file manually to the Pico BOOTSEL drive:

```text
firmware/apps/calibration-station/target/thumbv6m-none-eabi/debug/examples/single_servo_output.uf2
```

## Connections and expected behavior

Keep GP0 unloaded: no servo or mechanical load is connected. Probe GP0 (physical pin 1) against Pico GND with an oscilloscope. Debug UART1 TX is GP20 (physical pin 26): connect to a 3.3 V TTL adapter RXD and common GND. Leave adapter TXD and power outputs disconnected. Use 115200 baud, 8N1 and no flow control; messages end in LF. USB powers and programs the Pico.

Open the terminal before reset to capture the first phase message. The test runs automatically on every boot.

| Phase | Duration | GP0 | LED | UART message |
| --- | --- | --- | --- | --- |
| Inactive | 2 seconds | Low | On | INFO Single output: inactive |
| Running | 5 seconds | 1500 us High, 20 ms period | Toggles every 500 ms | INFO Single output: 1500 us running |
| Stopped | Until reset | Low | On | INFO Single output OK: stopped |

The firmware asserts inactive state and stopped DMA before startup and after stopping, checks for output faults continuously and calls stop twice to verify repeated-stop behavior. Scope observation is required to confirm physical pin behavior; UART success alone does not measure pulse timing.

## Verification

The owner confirmed the expected GP0 behavior and UART1 phase output, then reported 854 tests with zero failures and all 23 verify checks passing. No additional precision measurement, physical servo operation or EEPROM operation is claimed. CI for this addition has not yet been reported.

The [normal station application](../README.md) remains a separate startup/logging binary. Hardware-test builds do not change its behavior.
