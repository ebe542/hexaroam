# Calibration-station hardware tests

Manual Pico firmware examples, registered in the station Cargo.toml behind `hardware-tests`. They are built separately and are not executed by host tests or CI.

## Single-channel output

Source: [single_servo_output.rs](single_servo_output.rs).

The test uses the existing PIO/DMA switching-output driver directly, without ServoBank, joint angles or calibration data. One PIO state machine drives GP0. Two static four-word buffers hold the single-channel plans: one High interval and one Low interval, each encoded as a GPIO mask and duration.

Build from the repository root:

```bash
python tools/dev.py hardware-build --app calibration-station
```

The task currently builds both registered examples and generates their linker maps, disassemblies and UF2 files. A successful run has five build/report/conversion checks. Copy this file manually to the Pico BOOTSEL drive:

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

The owner confirmed the expected GP0 behavior and UART1 phase output, then reported 854 tests with zero failures and all 23 verify checks passing. No additional precision measurement, physical servo operation or EEPROM operation is claimed. CI was subsequently reported green for this direct-output test.

The [normal station application](../README.md) remains a separate startup/logging binary. Hardware-test builds do not change its behavior.

## Controlled single-channel output

Source: [controlled_servo_output.rs](controlled_servo_output.rs).

This separate example exercises CalibrationControl through the application-owned PioPulseBackend adapter and the existing SwitchingOutput PIO/DMA driver. The direct single_servo_output example remains unchanged as lower-layer coverage. UART request decoding and COBS are not involved in this test.

Use the build command above, then copy this UF2 manually to the Pico BOOTSEL drive:

```text
firmware/apps/calibration-station/target/thumbv6m-none-eabi/debug/examples/controlled_servo_output.uf2
```

Use the same unloaded GP0 and UART1 connections described above. Open the terminal before reset. The test runs automatically on every boot.

| Phase | Duration | GP0 | LED | UART message |
| --- | --- | --- | --- | --- |
| Prepared | 2 seconds | Low | On | INFO Controlled output: prepared; inactive |
| Running | 3 seconds | 1500 us High, 20 ms period | Toggles every 500 ms | INFO Controlled output: 1500 us running |
| Updated | 3 seconds | Changes to 1700 us High at a period boundary | Blinking | INFO Controlled output: 1700 us accepted |
| Stopped | Until reset | Low | On | INFO Controlled output OK: stopped |

The prepared state must not start output. Enabling without a prepared pulse is rejected before the test prepares 1500 us. During the update phase a 2001 us request is rejected and the last accepted value remains 1700 us. The acceptance message does not mean that the replacement pulse is already visible on GP0. Disabling is called twice; the prepared value remains available after stop. Service runs continuously and the test asserts that OUTPUT_ERROR is clear.

On 2026-11-10 the owner reported all five station hardware-build checks passing, confirmed the expected UART messages and pulse behavior, and subsequently reported 895 tests with zero failures and all 24 verify checks passing. This confirms the exercised service/adapter/PIO path on an unloaded GPIO. It does not establish physical servo behavior, UART command processing, forced Busy/error-path behavior on hardware or a long-duration timing result. CI for the adapter/test revision is pending.
