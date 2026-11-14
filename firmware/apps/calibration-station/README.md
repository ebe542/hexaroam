# Calibration station

Independent Raspberry Pi Pico (RP2040) application for a single-servo calibration bench. It accepts raw pulse widths from a PC and controls GP0 through PIO/DMA. Physical servo hookup, angle measurements and EEPROM programming are subsequent steps.

## Current behavior

The normal application connects UART0, COBS/CRC32 framing, the station command handler, CalibrationControl and PioPulseBackend. At boot the output is inactive, no pulse is prepared and status is zero. SET_PULSE prepares a valid value; OUTPUT_ENABLE(1) starts it. Initial limits are 1000 through 2000 us inclusive, with a 20000 us period. Disable stops output and retains the prepared pulse. See the [protocol contract](protocol.md) for replies, flags and errors.

UART1 provides one startup line:

```text
INFO Calibration station ready
```

The LED toggles every 500 ms to indicate loop activity, independently of servo-output state. Logging is serviced without waiting, with no periodic status messages. UART receive errors are logged; an output fault is logged once. Panic output uses the bounded UART1 handler. USB is used for power/programming, not protocol or debug serial communication.

## Application-owned modules

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Clock, pin and timer initialization; transfers resources to app::run |
| `src/app.rs` | UART/PIO assembly and nonblocking application loop |
| `src/lib.rs` | Library entry point for hardware-independent modules |
| `src/command.rs`, `src/test_command.rs` | Station request decoding, 12 host tests |
| `src/control.rs`, `src/test_control.rs` | Raw-pulse service and PulseBackend contract, 14 host tests |
| `src/command_handler.rs`, `src/test_command_handler.rs` | Dispatch and ACK/NACK content, 14 host tests |
| `src/frame_processor.rs`, `src/test_frame_processor.rs` | Bounded transport/framing/service integration, 14 host tests |
| `src/pio_pulse_backend.rs` | Embedded GP0 adapter for PIO0/SM0, DMA CH0 and two four-word buffers |
| `src/uart_transport.rs` | Nonblocking UART byte transport |
| `src/uart_logging.rs`, `src/panic_uart.rs` | Debug logger backend and bounded panic output |

Station-specific command/control code lives here; shared COBS/CRC32 framing, transport interfaces and common protocol error types remain in servo-protocol. Controller-specific code is unchanged. UART and panic drivers are currently local copies from the controller application.

Embedded dependencies are restricted to ARM bare-metal targets. Host tests compile only the station library and its hardware-independent dependencies. The build script supplies the Pico memory layout only for bare-metal targets. CRC calculation reuses SoftwareCrc32 from servo-storage; this dependency does not enable EEPROM or journal writes in the station.

## Loop and timeout behavior

Each poll services output, then pending transmission, then up to 32 received bytes. Up to 32 response bytes are accepted by the UART per poll. New responses are transmitted on a later poll; pending transmission blocks reception but not output servicing. Invalid COBS/CRC frames never reach the command handler. Receive errors discard incomplete framing; transmission retains an unaccepted suffix.

The initial receive-idle timeout is 100 ms. It is checked only after a read found no available input, not after receive-budget exhaustion or while a response blocks reception. It discards partial framing without changing pending responses. This is a bench setting, not a guaranteed response deadline. No delay loop is used; output, logging and LED work continue.

## Build and upload

Run from the repository root:

```bash
python tools/dev.py build --app calibration-station
python tools/dev.py uf2 --app calibration-station
```

The first command builds ELF; the second builds and converts through the Cargo runner. Neither uploads firmware. Copy this file manually to the Pico BOOTSEL drive:

```text
firmware/apps/calibration-station/target/thumbv6m-none-eabi/debug/calibration-station.uf2
```

The local Cargo configuration selects thumbv6m-none-eabi. memory.x includes BOOT2 and uses the Pico flash without the controller's historical journal reservation. Commit the application's Cargo.lock.

## Wiring and PC checks

Initially keep GP0 unloaded; do not connect a servo for these checks.

| Signal | Pico pin | Connection |
| --- | --- | --- |
| Servo signal | GP0, physical 1 | Oscilloscope probe |
| Protocol TX | GP16, physical 21 | Adapter TTL RXD |
| Protocol RX | GP17, physical 22 | Adapter TTL TXD |
| Debug TX | GP20, physical 26 | Separate adapter TTL RXD, when available |
| Ground | GND | Common reference for adapters and scope |

Use 3.3 V TTL, not RS232/RS485. UART0 and UART1 use 115200 baud, 8N1 and no flow control. Keep adapter power outputs disconnected; the debug adapter TXD is unused. A single adapter can run the protocol suite without simultaneously displaying debug logs. Open the debug terminal before reset to capture the single startup line. The current Windows setup uses COM8 for UART0 protocol and COM7 for UART1 debug. Close any other client of the protocol COM port.

At boot GP0 must remain Low and the LED must blink. Run the [13-scenario UART suite](hardware_tests/suites/README.md) after resetting the Pico. The suite drives the normal application; the two separate manual output examples are documented in the [hardware-test guide](hardware_tests/README.md).

## Verification

```bash
python tools/dev.py verify
python tools/dev.py hardware-build --app calibration-station
```

For just the 54 station host tests:

```bash
(cd firmware/apps/calibration-station && cargo test --lib --locked --target host-tuple)
```

The shared verify/test tasks explicitly select the host target for these tests. doc documents the station library. hardware-build separately creates ELF, UF2, maps and disassembly for the two manual examples; it does not upload or execute them.

The owner reported all 14 frame-processor tests passing, all 13 UART hardware scenarios passing and full local verification of 909 tests with zero failures and all 24 checks. Earlier direct and controlled PIO examples confirmed the unloaded GP0 pulse sequences. The owner subsequently confirmed the separate slow scope suite: one passed scenario and correct observed pulses through preparation, start, update, rejected update, stop and restart. See the [scope procedure](hardware_tests/suites/README.md#full-chain-scope-suite). Physical servo operation and EEPROM access remain pending. CI was subsequently reported green for the framing/runtime/UART-suite revision.

See [development tools](../../../tools/README.md#application-selection) and the [firmware overview](../../README.md).

## PC calibration documents

The separate [PC data model and JSON format](../../../docs/calibration-data.md) now support validated angle/pulse pairs, draft documents and a completeness check for all configured channels. This data model lives under tools/ and does not change the firmware protocol. The owner reported 933 tests and all 24 checks passing after adding its 24 Python tests. The [installed PC console](../../../docs/calibration-console.md) now handles interactive acquisition over UART0 and JSON saving. Transfer of calibration datasets and EEPROM programming remain future steps; CI for this PC-model revision is pending.

The PC client hardware check is [hardware_tests/check_client.py](hardware_tests/check_client.py). The owner confirmed all six operations and the interactive console smoke check with synthetic measurements; this did not use an attached physical servo. Reusable PC modules are installed as hexaroam_tools, and the script no longer changes sys.path. Follow the console guide for COM8 protocol and COM7 debug setup.
