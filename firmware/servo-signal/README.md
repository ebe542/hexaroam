# Shared servo signal logic

`hexaroam-servo-signal` is a `no_std` Rust library without external dependencies. It validates pulse requests and enforces a single pending transfer. PIO instructions, timing corrections, and FIFO access remain in the [Pico driver](../pico/src/drivers/servo_pio.rs).

## API

- PulseLimits::new(min_us, max_us) rejects reversed limits with LimitsError::MinExceedsMax. Equal limits are valid.
- contains checks inclusive bounds; min_us and max_us expose read-only endpoints.
- PulseTransport supplies is_empty and write. The write argument is microseconds, not an encoded data word; true means accepted, false means no write occurred.
- submit_pulse validates first, then checks for an empty transfer slot, then attempts one write. It returns PulseError::OutOfRange or PulseError::Busy on rejection.

Validation takes precedence over occupancy. Limits are configuration supplied by the application, not servo-specific constants. A hardware backend must independently ensure that its configured limits are representable.

The transport has one writer. A pending request must not be overwritten or followed by more queued requests. A successful return confirms acceptance only, not physical execution or actual servo position.

## Host tests

From the repository root, run in Git Bash on Windows or Bash on Ubuntu:

```bash
cd firmware/servo-signal
cargo test --lib
```

This sibling package does not inherit the Pico package's local target configuration. Run tests from its directory so they use the host target. The library remains no_std; Cargo's test harness runs on the host.

Unit tests live in src/tests.rs, included by the cfg(test) module declaration in src/lib.rs. They use a test transport with one Option slot and a write counter, without real hardware.

Six tests cover:

1. Inclusive endpoints and rejection just outside the configured range.
2. Reversed limits.
3. Equal limits allowing one value.
4. Invalid requests causing no writes.
5. Occupied transport preserving its pending value without writes.
6. Acceptance, rejection while pending, simulated consumption, and renewed acceptance.

The owner reported six passing tests on Windows on 2026-10-14. Ubuntu execution is not verified. The fallback where write returns false after an empty check, error precedence with an occupied transport and invalid input, and Pico initialization error paths are not specifically covered by these tests.

## Pico integration

The Pico firmware uses a local path dependency. Its private adapter handles actual FIFO access and PIO encoding. See the [firmware guide](../README.md) for build, UF2 conversion, and hardware observations.

Host tests do not prove FIFO register behavior, physical pulse timing, or actual servo operation. The owner separately confirmed a successful embedded build and apparently unchanged three-pulse behavior on the Pico.

Each package currently has its own Cargo.lock. No shared Cargo workspace is configured.

## Complete signal-plan output interface

`SignalPlanOutput` defines submission of a borrowed SignalPlan and nonblocking servicing. A backend accepts the whole plan or rejects it without partial acceptance. It must copy or encode the plan during the call rather than retain the borrowed reference. Acceptance is not confirmation of physical activation. `service()` advances backend work without waiting for a full signal period or issuing a new target request.

The interface has no Pico dependency. SwitchingOutput in servo-pio implements it; ServoSignalOutput in servo-control uses it to adapt ServoId-ordered pulse batches to complete GPIO plans. This leaves PIO encoding and DMA ownership inside the hardware driver. See the [adapter guide](../servo-control/README.md#signal-plan-output-adapter).

On 2026-10-26 the owner reported 14 passing adapter tests and a full verification total of 169 tests. Those fourteen tests belong to servo-control; the signal library's existing test count is unchanged. The new angle-command hardware demo remains pending.
