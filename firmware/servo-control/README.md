# Servo control

`hexaroam-servo-control` is a hardware-independent no_std library. It provides Angle, two-point calibration, calibrated Servo commands through PulseOutput, and ServoBank for validating and submitting multiple targets. It has no external dependencies and is now used by the Pico firmware through its private PIO driver.

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
| src/angle/tests.rs | Angle unit tests |
| src/calibration.rs | Calibration validation and integer interpolation |
| src/test_calibration.rs | Calibration unit tests |
| src/servo.rs | PulseOutput, Servo, and error forwarding |
| src/test_servo.rs | Servo wrapper and validation unit tests |
| src/servo_bank.rs | Servo IDs, targets, batch validation, and submission |
| src/test_servo_bank.rs | Servo bank unit tests |

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

Use python3 on Ubuntu if python is unavailable. The shared task runner formats/checks all three Rust packages and tests both libraries; see the [task guide](../../tools/README.md).

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

PulseOutput uses an associated Error type and contains no PIO-specific types. The shared servo-control package does not depend on the signal package or Pico HAL. Actual output-backend pulse limits remain independently enforced; the wrapper constructor does not validate calibration against backend limits.

Five wrapper tests cover construction without output, a calibrated command, rejection of angles below and above the range without output, forwarding Busy without retry, and successive commands with their own pulse widths. The test output records calls and accepted values using core::cell::Cell.

On 2026-10-14 the owner reported the full local check passing: 21 control tests, 6 signal tests, 3 Python tests, Rust formatting, and the existing Pico build. The 21 control tests comprise 4 Angle, 12 calibration, and 5 Servo tests. The previous calibration/task-runner revision had a green GitHub Actions run; the Servo wrapper subsequently passed CI. The current Pico integration awaits its first reported remote run.

The owner subsequently reported a successful full check and UF2 upload for Pico integration. ServoPio implements PulseOutput; main wraps it in Servo and uses synthetic calibration points of -45 degrees / 1000 us and +45 degrees / 2000 us. The application cycles through -45, 0, and +45 degrees approximately once per second. On 2026-10-14, the owner reported that the uploaded firmware runs as expected, without an attached servo. This confirms the integrated angle-to-signal path, not physical joint position or exact pulse timing.

## Servo bank

`Servo::validate_angle()` checks a target using the existing calibration without calling the output. `ServoChannel` exposes this check and `set_angle()` through a common interface, allowing channels with different concrete output types to be managed together.

`ServoBank<'a, E, N>` borrows a fixed-size array of mutable channel references. All channels share the output error type E; their concrete types may differ. The bank requires no heap allocation. `ServoId(0)` identifies the first configured channel, and subsequent IDs follow array order.

`set_targets(&[ServoTarget])` is the only batch command API. A single target, equal angles for multiple channels, and different angles all use this method. An empty slice succeeds without output; omitted channels receive no command.

The bank validates the entire request before submitting any command. Unknown IDs, duplicate IDs, or angles outside the selected channel's calibration reject the request without calling any output. Calibration errors include the affected servo ID. This guarantee covers bank-level ID and angle validation; backend-specific pulse limits remain independently enforced during submission.

After successful validation, commands are submitted in the supplied order. The first command error stops submission and is returned with its servo ID. There is no retry or rollback: an earlier command may already have been accepted. Success means that all outputs accepted their commands; it does not guarantee simultaneous activation or measured joint positions. Existing PWM signals continue with their previous values when validation rejects a request.

Two additional Servo tests cover valid boundary/center validation and rejected out-of-range angles without output. Ten bank tests cover construction, empty requests, a single selected channel, ID-based assignment, individual calibrations for equal angles, unknown and duplicate IDs, invalid angles after a valid target, and early/late output failures. The tests use real Servo instances with recorded mock outputs.

On 2026-10-16, the owner reported all ten bank tests passing and a successful full local check with 80 tests, Rust formatting checks, and the Pico build. The control package now has 33 tests. The Pico application does not yet use ServoBank; synchronized PIO startup and bank integration are the next firmware step. Remote CI for this library revision has not yet been reported.

## Next step

Integrate ServoBank into the Pico application and prepare both PIO channels before starting them synchronously. Prepare and verify the electrical servo test setup before connecting a servo. Replace synthetic calibration with measured joint calibration only after hardware commissioning.
