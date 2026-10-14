# Servo control

`hexaroam-servo-control` is a hardware-independent no_std library. Its current scope is the Angle value type; calibration and angle-to-pulse conversion are not implemented yet. It has no external dependencies and is not yet used by the Pico firmware.

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

The previous GitHub Actions workflow was reported green. The workflow already calls the shared task runner, so the new package will be included on the next push; a remote result for this revision has not yet been reported.

## Next step

Design two-point calibration, direction handling, overflow-safe interpolation, and rounding before implementation. Later calculations are planned to use i64 intermediates.
