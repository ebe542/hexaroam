# Servo control

`hexaroam-servo-control` is a hardware-independent no_std library. It provides the Angle value type and two-point angle-to-pulse calibration. It has no external dependencies and is not yet used by the Pico firmware.

## Angle representation

Angle stores a signed i32 number of centidegrees (hundredths of a degree). For example, 4525 represents 45.25 degrees and -1250 represents -12.50 degrees. No floating-point arithmetic is required. The 0.01-degree representation is not a claim about mechanical servo accuracy.

- `Angle::from_centidegrees(i32)` accepts every i32 value directly.
- `Angle::from_degrees(i32)` accepts whole degrees and returns Result. Multiplication by 100 is checked; overflow returns AngleError::Overflow.
- `as_centidegrees()` returns the stored value.
- Angles support equality and ordering, and implement Copy and Clone.

Fields are private. This type imposes no joint travel limits; calibration will define those separately.

## Files

| File | Purpose |
| --- | --- |
| src/lib.rs | no_std declaration and public exports |
| src/angle.rs | Angle and checked degree conversion |
| src/angle/tests.rs | Unit tests included only for test builds |

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

The owner reported a green GitHub Actions run for the Angle implementation. The current calibration changes await their first reported remote run.

## Two-point calibration

Calibration maps joint angles to pulse widths using two measured endpoints. The lower angle must be strictly below the upper angle. Pulse widths must be nonzero and different; increasing and decreasing pulse directions are supported.

pulse_width_us rejects targets outside the inclusive angle range. In-range values are linearly interpolated and rounded to the nearest whole microsecond, with exact ties going to the larger pulse width. Endpoints map exactly.

Angle differences use i64; nonnegative interpolation weights and the weighted sum use u64 to support the full i32 angle and u32 pulse ranges without overflow. Mathematical limits do not establish valid servo pulse ranges: actual calibration and output-backend constraints remain separate.

Twelve calibration tests cover construction, endpoints, midpoint, rejected out-of-range angles, rounding, decreasing pulse widths, and extreme inputs. Together with the four Angle tests, the owner reported 16 passing control tests on Windows. Implementation followed incremental red/green tests. No physical servo calibration or Pico integration has been performed.

Implementation: src/calibration.rs. Unit tests: src/test_calibration.rs, included with an explicit path attribute.

## Next step

Integrate calibrated angle commands with the existing signal layer after agreeing on the interface. Verify the combined workflow locally and in GitHub Actions.
