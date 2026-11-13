# Calibration-station UART suites

JSON suites run manually against the normal calibration-station firmware on a Pico. They are not host unit tests and are not executed by verify or CI. The existing Python runner handles encoding, exact response comparison and fail-fast reporting.

## Command suite

File: [uart_calibration_station.json](uart_calibration_station.json).

Build and convert the normal application from the repository root:

```bash
python tools/dev.py uf2 --app calibration-station
```

Upload calibration-station.uf2 from the application's debug target directory. Keep GP0 unloaded. Connect the protocol adapter in 3.3 V TTL mode: GP16 TX to adapter RXD, GP17 RX to adapter TXD and common GND. UART0 uses 115200 baud, 8N1, no flow control. Adapter power outputs remain disconnected. UART1 debug output is separate on GP20; no debug connection is required to run the suite. The current Windows setup uses COM8 for UART0 protocol and COM7 for UART1 debug; both use 115200 baud, 8N1. Port names are local setup examples, not firmware settings.

Restart the Pico before every invocation, including a selected case. Then run from the repository root:

```bash
python tools/uart_test.py --port COM8 --suite firmware/apps/calibration-station/hardware_tests/suites/uart_calibration_station.json
```

On Ubuntu use the corresponding port, for example /dev/ttyUSB0, and python3 if needed. Install optional dependencies with `python -m pip install -e ".[hardware]"` in the selected Python environment. Close other clients of the protocol port.

The explicit suite path is required because bare suite names currently default to the servo-controller suite directory. A single scenario can be selected with `--case active_update`.

## Fixture and scenarios

The suite's fixture_checks list uses ordinary step objects. Before any baseline it checks status=0/pulse=0, verifies enable without a pulse returns InvalidState and checks the state is unchanged. A nonzero prepared state requires a Pico restart before retrying; disabling alone does not clear the prepared pulse.

Before each case the suite disables output and prepares 1500 us. Every successful case finishes with output disabled. A failure aborts immediately and does not guarantee cleanup; reset the Pico after a failure or interruption. These tests can produce pulses and must run without an attached servo.

| Scenario | Coverage |
| --- | --- |
| prepared_status | Prepared pulse remains inactive |
| enable | Enable starts the prepared pulse |
| active_update | Active update to 1700 us |
| disable_reenable | Stop preserves the pulse and permits restart |
| inclusive_limits | Accept 1000 and 2000 us |
| out_of_range | Reject 999/2001 us and preserve accepted state |
| invalid_enable | Reject values other than 0/1 |
| invalid_lengths | Reject missing and extra parameters |
| unknown_command | NACK includes the original unknown command |
| bad_crc | Ignore damaged CRC and accept the next request |
| malformed_cobs | Recover after a truncated COBS block |
| receive_timeout | Discard incomplete reception after idle timeout |
| ignore_responses | Ignore incoming ACK/NACK without response loops |

Successful replies use a 1000 ms timeout followed by the runner's short settling interval. No-reply cases observe 200 ms, exceeding the 100 ms firmware receive-idle timeout. Stored request/response bytes include generated COBS/CRC32 and are checked against generation by the loader.

## Verification

The owner reported the fixture and all 13 scenarios passing: 13 passed, zero failed and zero not completed. Subsequent full local verification passed 909 tests with zero failures and all 24 checks. This is UART request/response evidence with the real PIO backend, not an oscilloscope measurement or physical servo test. The separate full-chain scope suite was subsequently confirmed as described below. CI for this revision is pending.

See the [station protocol](../../protocol.md) and [application guide](../../README.md).

## Full-chain scope suite

File: [uart_calibration_station_scope.json](uart_calibration_station_scope.json). It uses the same normal calibration-station firmware and fixture preflight as the command suite. Restart the Pico before invocation and observe unloaded GP0 against GND. No servo is connected.

```bash
python tools/uart_test.py --port COM8 --suite firmware/apps/calibration-station/hardware_tests/suites/uart_calibration_station_scope.json
```

| Observation window | Expected GP0 behavior |
| --- | --- |
| 2 seconds prepared | Low |
| 3 seconds after enable | 1500 us High, 20 ms period |
| 3 seconds after update | 1700 us High, 20 ms period |
| 3 seconds after rejected 2001 us request | Unchanged 1700 us High |
| 3 seconds after stop | Low |
| 3 seconds after re-enable | Retained 1700 us High |
| Final 2 seconds, then until another command/reset | Low |

These are observation windows; command exchange and status checks add some time between them. Empty zero delimiters provide no-action observation steps while the runner checks that no unexpected bytes arrive. The suite ends with output disabled and the 1700 us prepared value retained.

The runner's single scenario verifies ACK/NACK and status replies. Scope observation is separate evidence. The owner reported one passed scenario, zero failures and zero not completed, and explicitly confirmed correct pulses. Together with the earlier command suite this confirms the exercised UART0-to-PIO/DMA path, including preparation, start, update, rejection, stop and restart. No numerical precision trace, endurance run, physical servo motion or EEPROM operation is claimed. Full local verification before this scope-only suite addition passed 909 tests and all 24 checks; CI for the combined revision is pending.
