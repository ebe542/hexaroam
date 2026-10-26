# Logging

`hexaroam-logging` is a hardware-independent `no_std` library without external dependencies or heap allocation. It provides buffered runtime logging and a separate bounded writer for emergency output.

## Buffered runtime output

`Logger<O, CAPACITY>` owns a `ByteOutput` backend and a fixed-capacity byte ring buffer. `log(level, message)` queues a complete `LEVEL message\n` line. Supported levels are `Info`, `Debug`, and `Error`; output prefixes are uppercase. Line endings contain LF only. UTF-8 capacity is counted in bytes.

Logging does not transmit bytes. The application's main loop calls `service(byte_budget)` regularly. Each call attempts to transfer at most that many bytes and stops immediately when the backend is busy or reports an error. Only accepted bytes are removed. There are no logging interrupts or background tasks.

If a complete line does not fit, `log` returns `LogError::BufferFull`, leaves existing queued data unchanged, and increments a saturating dropped-message counter. `queued_bytes()` and `dropped_messages()` expose the current counters. An accepted log entry is queued, not necessarily physically transmitted.

`ByteOutput::try_write` must return immediately:

- `Ok(true)`: the byte was accepted.
- `Ok(false)`: the backend is busy; the byte was not accepted.
- `Err(error)`: the byte was not accepted.

For the Pico, [UartLogOutput](../pico/src/uart_logging.rs) owns an enabled HAL UART peripheral and writes through its nonblocking FIFO API. The current tests configure UART1 TX on GP20 at 115200 baud, 8N1. The UART FIFO accepting a byte does not mean it has already left the pin.

## Bounded direct output

`BoundedWriter` borrows a `ByteOutput` and implements `core::fmt::Write`, allowing formatted output without the runtime log buffer. Its attempt budget is shared across all `write_str` calls. Every attempt consumes budget, including retries while the backend is busy. Budget exhaustion returns `fmt::Error`; a backend error also exhausts the budget. Already accepted bytes are not rolled back. No newline is added automatically.

The budget limits write attempts, not elapsed milliseconds. It relies on the backend returning immediately. This mechanism supports best-effort panic output when normal main-loop servicing has stopped.

## Pico panic handler

The [panic handler](../pico/src/panic_uart.rs) is selected by the normal firmware, fixed_plan, uart_panic, and the two restart hardware tests. Other hardware programs retain panic-halt unless they explicitly select this handler.

The handler disables interrupts, checks that UART1 is out of reset with its UART and transmitter enabled, and writes directly through its registers using a total budget of 1,000,000 write attempts. An initial LF separates the panic from potentially interrupted normal output. `PanicInfo` supplies the message and source location, including for failed `expect`, `unwrap`, and assertions. If UART1 is not enabled, output is skipped. Afterwards, the CPU stays in a permanent loop while peripheral clocks continue running, so accepted UART FIFO bytes can still be transmitted.

This direct access requires that core 1 and DMA do not write to UART1. The handler neither initializes UART1 nor drains the normal software log queue. Panic output can be incomplete. It does not implement servo shutdown or controlled PIO fault recovery.

See the [hardware-test guide](../pico/hardware_tests/README.md#uart-connection-and-terminal) for wiring and reproduction.

## Verification

From the repository root:

```bash
cargo test --manifest-path firmware/logging/Cargo.toml
python tools/dev.py verify
python tools/dev.py hardware-build
```

The development runner includes this library in formatting, checks, and host tests. `verify` builds the normal Pico firmware; `hardware-build` separately builds and converts the manual hardware programs to UF2.

On 2026-10-17, the owner reported 17 passing logging unit tests and a full runner total of 145 passing tests. On the physical Pico, Info, Debug, and Error lines were received in HTerm, including after switching to LF-only line endings. The panic test emitted its message after approximately five seconds and left the LED permanently on. Manual hardware programs are excluded from the unit-test total. The owner subsequently confirmed UART output alongside servo pulses in the normal demo, and state-transition logging is also used in the recovery tests. Early panic before UART initialization and stalled UART output have not been separately tested on hardware. The owner reported green CI for the earlier logging revision.
