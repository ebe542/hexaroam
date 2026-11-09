# COBS command hardware tests

## Build and fixture

The separate `uart_commands` firmware connects UART0, COBS/CRC32, CommandProcessor, CommandService, a real two-servo ServoBank and PIO/DMA output. Calibration is synthetic in RAM: -45/0/+45 degrees maps to 1000/1500/2000 us on both channels. There is no EEPROM or flash access. The normal firmware and register suites remain separate.

```bash
python tools/dev.py hardware-build
```

Flash `firmware/apps/servo-controller/target/thumbv6m-none-eabi/debug/examples/uart_commands.uf2`. Reset before each suite invocation. Expect `UART COBS commands ready` on debug UART GP20 at 115200 baud. LED blinking indicates main-loop activity, not active servo output.

Connect a 3.3-V TTL adapter: TX to GP17, RX to GP16, common GND, 115200 baud, 8N1 without flow control. Remove any GP16/17 loopback. Observe unloaded GP0/GP1 against Pico GND; do not connect servos. Close HTerm on the protocol port before starting the runner. A second receiver can monitor GP20 independently.

Install optional hardware dependencies in the active environment if needed:

```bash
python -m pip install -e ".[hardware]"
```

## Automated response suite

```bash
python tools/uart_test.py --port COM6 --suite uart_commands
```

There are 23 cases. Each begins and ends with disable plus Status 1. Preflight requires Status 1 before modifying commands. Status 3 means calibrated and enabled without running; Status 7 adds running. No version/count readback is available. The documented count of two channels identifies the fixture, not a queried value. A physically different fixture with the same Status cannot be identified by preflight alone.

The suite covers valid commands and partial updates, initial-pose requirements, parameter/range errors, unknown commands, ignored incoming replies, CRC/COBS errors, overflow, empty frames, noise, receive timeout, byte loss, repeated enable, restart and a maximum-size frame. Expected full wire bytes are stored alongside unframed content and validated by the runner. CRC corruption occurs before COBS encoding. Raw steps bypass encoding deliberately. No-response observation windows are 200 ms, exceeding the fixture's 100-ms receive-idle timeout. Successful expected replies retain the existing 20-ms trailing-byte check.

Each case's final disable prevents output remaining active after a successful run. A failed/interrupted run may stop before cleanup: reset the Pico and check Low outputs before retrying. Do not assume an error automatically disabled output.

## Separate scope suite

```bash
python tools/uart_test.py --port COM6 --suite uart_commands_scope
```

This intentionally lasts at least ten seconds, with two seconds for each observation:

| Phase | Status | GP0 | GP1 |
| --- | --- | --- | --- |
| Enabled without initial pose | 3 | Low | Low |
| Complete initial pose | 7 | 1000 us | 2000 us |
| Partial update of Servo 0 | 7 | 1500 us | 2000 us |
| Rejected batch with invalid Servo 1 angle | 7 | Unchanged | Unchanged |
| Disabled | 1 | Low | Low |

During active phases, verify a 20-ms period and synchronous rising edges. ACK/Status success alone does not verify actual pulses or unchanged physical output after rejection. The deliberate pauses transmit only a zero delimiter; no command or reply is expected. Electrical startup must also show no bootstrap pulses, as separately tested by `inactive_output`.

## Framing selection and verification

These suites set `"framing": "cobs_crc32"`: zero start delimiter, COBS(content + CRC32 little endian), zero end delimiter. Old suites omit the field and retain `register_crc32`. The same JSON step fields and CLI are used. Requests and expected replies are built with the selected framing; ordinary transport execution is unchanged. No simulated serial-port tests are introduced.

Owner-reported validation before suite creation: 665 tests and all 17 checks passed, all 59 hardware-build checks passed, and the separate inactive-output hardware behavior was confirmed. On 2026-11-06 the owner subsequently reported all 23 uart_commands cases passing on real hardware, then Fixture check and scope_motion passing (1 case, no failures) and confirmed the signals matched the documented phases. This establishes the response suite and manual scope behavior separately. Final project verification after the Python runner/suite changes was reported passing: 665 tests, no failures, and all 17 checks. CI for this combined revision has not yet been reported. No numerical scope capture or sustained-throughput result is claimed. The earlier register-protocol throughput timeout remains unresolved.
