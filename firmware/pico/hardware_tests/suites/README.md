# UART hardware test suites

JSON suites in this directory describe tests against real Pico hardware. They are started manually and are not part of host verification or CI.

The runner resolves a bare suite name such as `uart_registers` or `uart_registers.json` in this directory, independently of the working directory. An explicit relative or absolute JSON path selects a suite elsewhere.

`uart_registers.json` targets `firmware/pico/hardware_tests/uart_registers.rs`. Its verification fields record the actual verification status; moving the suite does not constitute a hardware test.

From the repository root, with the hardware dependencies installed and the matching firmware flashed:

```bash
python tools/uart_test.py --port COM5 --suite uart_registers
```

See [UART hardware suite instructions](../../../../tools/README.md#uart-hardware-suites) for Windows/Ubuntu setup, wiring, port selection and individual scenarios.
