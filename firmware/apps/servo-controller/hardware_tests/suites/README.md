# UART hardware test suites

JSON suites in this directory describe tests against real Pico hardware. They are started manually and are not part of host verification or CI.

The runner resolves a bare suite name such as `uart_registers` or `uart_registers.json` in this directory, independently of the working directory. An explicit relative or absolute JSON path selects a suite elsewhere.

`uart_registers.json` targets `firmware/apps/servo-controller/hardware_tests/uart_registers.rs`. Its verification fields record the actual verification status; moving the suite does not constitute a hardware test.

From the repository root, with the hardware dependencies installed and the matching firmware flashed:

```bash
python tools/uart_test.py --port COM5 --suite uart_registers
```

See [UART hardware suite instructions](../../../../../tools/README.md#uart-hardware-suites) for Windows/Ubuntu setup, wiring, port selection and individual scenarios.

## Calibration persistence

`uart_calibration_persistence.json` targets the separate flash-writing firmware of the same name. Reset before each complete three-case run:

```bash
python tools/uart_test.py --port COM6 --suite uart_calibration_persistence
```

It persists synthetic A, repeats A and persists changed B. The firmware checks journal data, sequence and backend counters. See the [procedure and separate restart verification](../../../../../docs/uart-calibration-persistence.md). The owner reported the build passing 53 checks and subsequently all three cases passing on hardware after restart on 2026-11-04. A subsequent boot confirmed fixture B at sequence 28, slot 11.

## COBS command protocol

`uart_commands.json` contains 23 response/state/recovery cases for `uart_commands.rs`. `uart_commands_scope.json` is a separate one-case suite with five two-second observation phases. Both set `"framing": "cobs_crc32"`; suites without this field retain register framing.

```bash
python tools/uart_test.py --port COM6 --suite uart_commands
python tools/uart_test.py --port COM6 --suite uart_commands_scope
```

Reset before each invocation. Use unloaded GP0/GP1 only. See [full instructions and verification record](../../../../../docs/uart-command-tests.md). A failed run does not guarantee output cleanup; reset before retrying.

On 2026-11-06 the owner reported all 23 COBS command cases passing, followed by the separate scope case passing with correct signal observations.
