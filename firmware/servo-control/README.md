# Servo control

`hexaroam-servo-control` is a hardware-independent no_std library. It provides Angle, two-point calibration, calibrated Servo commands through PulseOutput, and ServoBank for validating and submitting complete pulse batches. It depends on the local servo-signal library for its signal-plan adapter and has no Pico HAL dependency. The current PIO/DMA demo still uses pulse widths directly; the bank and adapter have not yet been exercised together on the Pico.

## Angle representation

Angle stores a signed i32 number of centidegrees (hundredths of a degree). For example, 4525 represents 45.25 degrees and -1250 represents -12.50 degrees. No floating-point arithmetic is required. The 0.01-degree representation is not a claim about mechanical servo accuracy.

- `Angle::from_centidegrees(i32)` accepts every i32 value directly.
- `Angle::from_degrees(i32)` accepts whole degrees and returns Result. Multiplication by 100 is checked; overflow returns AngleError::Overflow.
- `as_centidegrees()` returns the stored value.
- Angles support equality and ordering, and implement Copy and Clone.

Fields are private. Angle imposes no joint travel limits; Calibration defines those separately.

## Files

| File | Purpose |
| --- | --- |
| src/lib.rs | no_std declaration and public exports |
| src/angle.rs | Angle and checked degree conversion |
| src/test_angle.rs | Angle unit tests |
| src/calibration.rs | Calibration validation and integer interpolation |
| src/test_calibration.rs | Calibration unit tests |
| src/servo.rs | PulseOutput, Servo, and error forwarding |
| src/test_servo.rs | Servo wrapper and validation unit tests |
| src/servo_bank.rs | PulseBankOutput, servo IDs, transactional batch submission, and accepted commands |
| src/test_servo_bank.rs | Servo bank unit tests |
| src/servo_signal_output.rs | GPIO mapping, complete-plan adapter, and configuration checks |
| src/test_servo_signal_output.rs | Adapter and bank-to-adapter tests |

## Verification

From the repository root:

```bash
python tools/dev.py verify
```

To run only this package's tests:

```bash
cd firmware/servo-control
cargo test --lib --locked
```

Use python3 on Ubuntu if python is unavailable. The shared task runner includes this package in formatting, code checks, and host tests; see the [task guide](../../tools/README.md).

Four Angle tests cover signed centidegree values including i32 extremes, conversion of negative/zero/positive whole degrees, representable whole-degree boundaries, and rejection beyond both boundaries.

On 2026-10-14 the owner reported all four Angle tests and the expanded local check passing on Windows: formatting of three packages, six signal tests, four angle tests, and the existing Pico build. At that earlier stage the library was not yet an embedded dependency; the later integration is recorded below.

The owner reported green GitHub Actions runs for Angle and subsequently calibration/task-runner changes. The owner subsequently reported a green workflow for the Servo wrapper; the current Pico integration awaits its first reported remote run.

## Two-point calibration

Calibration maps joint angles to pulse widths using two measured endpoints. The lower angle must be strictly below the upper angle. Pulse widths must be nonzero and different; increasing and decreasing pulse directions are supported.

pulse_width_us rejects targets outside the inclusive angle range. In-range values are linearly interpolated and rounded to the nearest whole microsecond, with exact ties going to the larger pulse width. Endpoints map exactly.

Angle differences use i64; nonnegative interpolation weights and the weighted sum use u64 to support the full i32 angle and u32 pulse ranges without overflow. Mathematical limits do not establish valid servo pulse ranges: actual calibration and output-backend constraints remain separate.

Twelve calibration tests cover construction, endpoints, midpoint, rejected out-of-range angles, rounding, decreasing pulse widths, and extreme inputs. Together with the four Angle tests, the owner reported 16 passing control tests on Windows. Implementation followed incremental red/green tests. No physical servo calibration has been performed; Pico integration was subsequently verified with synthetic calibration data.

Implementation: src/calibration.rs. Unit tests: src/test_calibration.rs, included with an explicit path attribute.

## Calibrated servo commands

Servo<O> owns a Calibration and an output implementing PulseOutput. Construction stores these values without issuing an output command. set_angle converts the target through the calibration and calls the output exactly once with the resulting microseconds.

ServoError::Calibration wraps conversion errors. Invalid angles do not call the output. ServoError::Output preserves the backend error, including Busy, without automatic retry or buffering. Ok confirms output acceptance only, not pulse activation or measured joint position.

PulseOutput uses an associated Error type and contains no PIO-specific types. The single-servo wrapper does not use signal-plan types. The package now depends on servo-signal for the separate bank adapter, but not on the Pico HAL. Actual output-backend pulse limits remain independently enforced; the wrapper constructor does not validate calibration against backend limits.

Five wrapper tests cover construction without output, a calibrated command, rejection of angles below and above the range without output, forwarding Busy without retry, and successive commands with their own pulse widths. The test output records calls and accepted values using core::cell::Cell.

On 2026-10-14 the owner reported the full local check passing: 21 control tests, 6 signal tests, 3 Python tests, Rust formatting, and the existing Pico build. The 21 control tests comprise 4 Angle, 12 calibration, and 5 Servo tests. The previous calibration/task-runner revision had a green GitHub Actions run; the Servo wrapper subsequently passed CI. The current Pico integration awaits its first reported remote run.

The owner subsequently reported a successful full check and UF2 upload for Pico integration. ServoPio implements PulseOutput; main wraps it in Servo and uses synthetic calibration points of -45 degrees / 1000 us and +45 degrees / 2000 us. The application cycles through -45, 0, and +45 degrees approximately once per second. On 2026-10-14, the owner reported that the uploaded firmware runs as expected, without an attached servo. This confirms the integrated angle-to-signal path, not physical joint position or exact pulse timing.

## Servo bank

`ServoBank<O, N>` owns N calibrations, N optional last-accepted angle commands, and one backend implementing `PulseBankOutput`. It requires no heap allocation and contains no GPIO, PIO, or Pico types. `ServoId(0)` identifies the first configured calibration; subsequent IDs follow array order. The single-servo `Servo` and `PulseOutput` interfaces remain available independently.

Construction performs no output and initializes every accepted command to None. `commanded_angle(id)` returns that servo's last accepted Angle, or None before initialization. Unknown IDs return UnknownServo. These are command records, not physical position feedback or confirmation of GPIO activation.

`set_targets(&[ServoTarget])` handles a single changed servo, equal angles for several servos, or different angles. An empty slice succeeds without output or initialization. The first nonempty request must contain every servo, so a complete pulse batch can be formed. Missing initial values return UninitializedServo. Rejected partial initial requests do not accumulate changes.

For each nonempty request, the bank copies its accepted angles into a local candidate, rejects unknown or duplicate IDs, and validates every requested angle against its own calibration. It then converts the complete candidate into a fixed-size pulse-width array in ServoId order. Once initialized, omitted servos retain their accepted targets and are included in every complete output batch. Calibration failures identify the affected servo.

The bank submits this entire array exactly once through `PulseBankOutput::submit_pulses(&[u32])`. The backend must consume or copy the borrowed slice before returning; it cannot retain the slice for later DMA access. A failed submission must not accept part of the new batch. Only success commits the candidate angles. Busy, Faulted, and other backend errors are preserved through ServoBankError::Output and leave previously accepted commands unchanged. There is no retry or hidden accumulation of rejected targets.

This replaces the former sequential bank implementation. The bank no longer borrows individual ServoChannel instances or reports per-channel output errors. ServoSignalOutput now maps the pulse array to configured GPIOs, constructs one complete SignalPlan, and submits it through SignalPlanOutput. SwitchingOutput implements that backend interface. Backend-specific pulse limits and plan capacity remain the adapter's responsibility.

### Verification record

On 2026-10-26 the owner reported all 16 new bank tests passing and full verification with 155 passed, 0 failed, 0 skipped, and 0 filtered out. The bank tests cover construction, empty requests, initial completeness, ServoId output order, partial updates, per-servo and decreasing calibration, unknown/duplicate IDs, invalid angles, Busy/Faulted preservation, and subsequent requests after rejection. The control package now contains 39 tests; the prior ten sequential-bank tests were replaced by sixteen transactional-bank tests.

These are host tests with a recording backend. No new Pico behavior, physical servo calibration, or simultaneous GPIO activation is established by this step. The owner subsequently reported green CI for this bank revision.

## Next step

Connect the tested bank and adapter to a separate Pico angle-command demo. Keep the single-state-machine PIO/DMA backend and synthetic calibration explicit. Actual servo commissioning and measured joint calibration remain separate hardware steps.

## Signal-plan output adapter

`ServoSignalOutput<O, N, PLAN_CAPACITY>` implements PulseBankOutput over a SignalPlanOutput backend. It owns the backend, a fixed GPIO list in ServoId order, shared pulse limits, and a period. Construction rejects an empty output list, GPIOs outside the generic 0..31 range, duplicate assignments, a zero period, and insufficient worst-case plan capacity. Require PLAN_CAPACITY >= N + 1: three entries for two servos or nineteen for eighteen. Equal pulse widths can use fewer entries, but configuration reserves the worst case.

Generic GPIO validation does not establish RP2040 pin availability. The RP2040 driver independently restricts GPIOs to 0..29, owns the actual pins, and checks that submitted plans match its configured output set and period. Board wiring and reserved pins remain application responsibilities.

`submit_pulses` requires exactly N values. It maps each value to its configured GPIO, builds a complete local SignalPlan, and submits the plan exactly once only after successful validation. The constructor checks a nonzero period; actual pulse positivity, shared limits, and pulse widths strictly below the period are checked when building each plan. PulseCountMismatch and SignalPlan errors do not call the backend. Backend errors, including Busy and Faulted, are wrapped as Output errors without retry. The bank commits its candidate angles only after this whole chain succeeds.

The borrowed pulse slice and local SignalPlan are not retained. SignalPlanOutput implementations must copy or encode the complete plan before returning. SwitchingOutput does this in its owned pending buffer; DMA does not read the temporary stack plan.

The main loop calls `bank.service()`, which delegates through PulseBankOutput, ServoSignalOutput, and SignalPlanOutput to the backend's service method. This services DMA without changing commanded angles or submitting a new target plan. It does not automatically restart faulted output.

### Adapter verification

On 2026-10-26 the owner reported all 14 adapter tests passing and full verification with 169 passed, 0 failed, 0 skipped, and 0 filtered out. The tests cover configuration rejection, complete pulse counts, ServoId-to-GPIO mapping, merging equal pulse endings, invalid pulses and period boundaries, backend error forwarding without replacement or retry, servicing, and preservation of bank commands after adapter-level validation failure. The control package now contains 53 host tests.

SwitchingOutput implements SignalPlanOutput by forwarding submission and servicing to its existing methods. The owner reported the full verification task successful; the new bank-to-adapter angle path has not yet been manually exercised on the Pico. CI for this adapter revision has not yet been reported.
