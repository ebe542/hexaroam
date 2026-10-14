# Servo control

`hexaroam-servo-control` is a hardware-independent no_std library. It provides Angle, two-point calibration, and calibrated Servo commands through a hardware-independent PulseOutput trait. It has no external dependencies and is not yet used by the Pico firmware.

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
| src/test_servo.rs | Servo wrapper unit tests |

## Verification

From the repository root:

```bash
python tools/dev.py check
```

To run only this package's tests:

```bash
cd firmware/servo-control
cargo test --lib --locked
```

Use python3 on Ubuntu if python is unavailable. The shared task runner formats/checks all three Rust packages and tests both libraries; see the [task guide](../../tools/README.md).

Four Angle tests cover signed centidegree values including i32 extremes, conversion of negative/zero/positive whole degrees, representable whole-degree boundaries, and rejection beyond both boundaries.

On 2026-10-14 the owner reported all four Angle tests and the expanded local check passing on Windows: formatting of three packages, six signal tests, four angle tests, and the existing Pico build. The new library is not yet an embedded dependency, so that build does not verify its integration into Pico firmware.

The owner reported green GitHub Actions runs for Angle and subsequently calibration/task-runner changes. Remote verification of the new Servo wrapper is pending.

## Two-point calibration

Calibration maps joint angles to pulse widths using two measured endpoints. The lower angle must be strictly below the upper angle. Pulse widths must be nonzero and different; increasing and decreasing pulse directions are supported.

pulse_width_us rejects targets outside the inclusive angle range. In-range values are linearly interpolated and rounded to the nearest whole microsecond, with exact ties going to the larger pulse width. Endpoints map exactly.

Angle differences use i64; nonnegative interpolation weights and the weighted sum use u64 to support the full i32 angle and u32 pulse ranges without overflow. Mathematical limits do not establish valid servo pulse ranges: actual calibration and output-backend constraints remain separate.

Twelve calibration tests cover construction, endpoints, midpoint, rejected out-of-range angles, rounding, decreasing pulse widths, and extreme inputs. Together with the four Angle tests, the owner reported 16 passing control tests on Windows. Implementation followed incremental red/green tests. No physical servo calibration or Pico integration has been performed.

Implementation: src/calibration.rs. Unit tests: src/test_calibration.rs, included with an explicit path attribute.

## Calibrated servo commands

Servo<O> owns a Calibration and an output implementing PulseOutput. Construction stores these values without issuing an output command. set_angle converts the target through the calibration and calls the output exactly once with the resulting microseconds.

ServoError::Calibration wraps conversion errors. Invalid angles do not call the output. ServoError::Output preserves the backend error, including Busy, without automatic retry or buffering. Ok confirms output acceptance only, not pulse activation or measured joint position.

PulseOutput uses an associated Error type and contains no PIO-specific types. The shared servo-control package does not depend on the signal package or Pico HAL. Actual output-backend pulse limits remain independently enforced; the wrapper constructor does not validate calibration against backend limits.

Five wrapper tests cover construction without output, a calibrated command, rejection of angles below and above the range without output, forwarding Busy without retry, and successive commands with their own pulse widths. The test output records calls and accepted values using core::cell::Cell.

On 2026-10-14 the owner reported the full local check passing: 21 control tests, 6 signal tests, 3 Python tests, Rust formatting, and the existing Pico build. The 21 control tests comprise 4 Angle, 12 calibration, and 5 Servo tests. The previous calibration/task-runner revision had a green GitHub Actions run; the new Servo wrapper revision awaits its first reported remote run.

The control library is not yet integrated into the Pico firmware, so the current Pico build does not establish embedded integration or physical angle control.

## Next step

Implement PulseOutput for the Pico output driver and connect a test calibration to angle commands in main. Review the integration before implementation and verify initially without an attached servo.
