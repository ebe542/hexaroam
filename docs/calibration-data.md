# Calibration documents

The PC-side data model in [tools/hexaroam_tools/calibration_data.py](../tools/hexaroam_tools/calibration_data.py) loads, validates and saves portable JSON calibration documents. It is independent of UART, Pico firmware and EEPROM access. The [interactive console](calibration-console.md) uses this model to capture and save measurements.

## Schema version 1

The following example contains fictional values for one servo; it is not a measured calibration:

```json
{
  "schema_version": 1,
  "servo_count": 1,
  "servos": [
    {
      "servo_id": 0,
      "lower_angle_centidegrees": -4500,
      "lower_pulse_us": 1000,
      "upper_angle_centidegrees": 4500,
      "upper_pulse_us": 2000
    }
  ]
}
```

All shown fields are required. Additional root or servo fields are rejected. Version must be the integer 1. The document root and each servo entry must be JSON objects; servos must be an array.

| Field | Meaning | Accepted values |
| --- | --- | --- |
| schema_version | JSON format version | Integer 1 |
| servo_count | Number of channels to calibrate | Integer 1 through 4294967295 |
| servo_id | Logical destination channel, not a hardware identity | Unique integer, 0 through servo_count - 1 |
| lower_angle_centidegrees | Measured lower reference angle | Signed 32-bit integer |
| lower_pulse_us | Pulse width at that angle | Integer 1 through 4294967295 |
| upper_angle_centidegrees | Measured upper reference angle | Signed 32-bit integer |
| upper_pulse_us | Pulse width at that angle | Integer 1 through 4294967295 |

A centidegree is 0.01 degree: -4500 means -45.00 degrees. Angles are relative to a defined reference at the calibration bench. They do not identify a mechanical joint or establish the installed robot's joint zero. Joint assignment, mounting offsets and joint motion limits are separate robot configuration concerns.

The lower angle must be strictly smaller than the upper angle. Pulse widths must be positive and different. Decreasing pulse widths are valid and represent reversed rotation direction. Strings, floating-point values, booleans and null are rejected for integer fields; Python bool must not pass as int.

Numeric widths match the existing Rust Calibration/binary fields. These checks do not imply that every representable value is usable by a physical servo, output period, EEPROM layout or board channel count. The station currently accepts only 1000 through 2000 us, with a 20000 us period. Hardware and deployment constraints must be checked when applying or exporting the document.

## Drafts and complete data

A draft can contain an empty list or a subset of configured servo IDs. Every present entry must already contain both valid measurement points. Missing or duplicate fields, duplicate IDs and IDs outside the configured range are rejected. Input order is unrestricted and is preserved during JSON round trips.

For example, an initial draft for 18 channels is:

```json
{
  "schema_version": 1,
  "servo_count": 18,
  "servos": []
}
```

load, from_json, save and to_json accept valid drafts. CalibrationData.require_complete() additionally requires every channel from 0 through servo_count - 1 exactly once. The future binary export/EEPROM operation must call this check before programming. Merely saving a file neither programs hardware nor changes controller calibration.

## Library API

| API | Behavior |
| --- | --- |
| ServoCalibration(...) | Validates one pair of angle/pulse measurement points |
| CalibrationData(servo_count, servos) | Validates count, tuple entries, unique IDs and configured range |
| data.require_complete() | Returns None if complete; otherwise raises ValueError |
| from_json(text) | Decodes and validates a version-1 document |
| to_json(data) | Produces indented JSON with explicit units and a trailing newline |
| load(path) | Reads a UTF-8 file and validates it |
| save(path, data) | Creates missing parent directories and writes the document as UTF-8 |

The dataclasses are frozen, and the Python model requires an immutable tuple of ServoCalibration entries. This keeps accepted values stable after validation. JSON syntax or calibration-validation errors raise ValueError; missing files raise FileNotFoundError and other filesystem errors propagate to the caller.

Install the package first with `python -m pip install -e .` in the selected environment. An example for creating an empty local draft, run from the repository root:

```bash
python - <<'PYTHON'
from pathlib import Path
from hexaroam_tools.calibration_data import CalibrationData, save

save(
    Path("local-data/calibration/servos.json"),
    CalibrationData(servo_count=18, servos=()),
)
PYTHON
```

This example writes an empty draft and replaces an existing file at that path. Personal measurement files belong under local-data/calibration/, which is excluded by the repository's local-data ignore rule. No real measured values are included in the example.

## Bench workflow

1. Select the servo and define a repeatable zero reference on the measuring fixture.
2. Adjust pulse width and record the measured lower angle with its accepted pulse value.
3. Record the upper angle and pulse in the same reference system.
4. Disable the output, validate the pair and save it to the draft document.
5. Repeat for other configured channels; require completeness before future export/programming.

The console accepts degrees with a decimal point or comma and at most two decimal places, using exact integer conversion to centidegrees. It displays two decimal places without floating-point arithmetic. The two points describe the linear mapping already used by Rust Calibration; they do not establish physical position feedback or compensate for unmeasured nonlinearities. Interactive acquisition is implemented in the console; binary export and EEPROM programming remain pending. No physical servo measurement has been reported.

## Verification

From the repository root:

```bash
python -m unittest discover -s tools -p test_calibration_data.py -v
python tools/dev.py verify
```

The 24 new Python tests cover valid and reversed mappings, integer bounds and types, drafts, duplicate/out-of-range IDs, completeness, strict JSON shape/version, JSON round trips and file loading/saving. Negative tests deliberately use invalid argument types; any type-checker casts in those tests do not weaken the production interface or change runtime values.

The owner reported full local verification of 933 tests with zero failures and all 24 checks. No UART, physical servo or EEPROM operation is performed by these data-model tests. The preceding station UART/PIO revision passed CI; CI for this data-model revision is pending.

## Capture sessions

[calibration_session.py](../tools/hexaroam_tools/calibration_session.py) manages a fresh two-point session for one selected servo. Existing measurements are not preloaded, so replacing a servo cannot silently mix an old point with a new one. Either point can be entered first. Invalid points or invalid pairs leave previous captured points unchanged. Decreasing pulse widths remain supported.

finish() returns a new document with the selected entry added or replaced; other servo entries and the original document remain unchanged. Both points are required, but the full document may still be an incomplete multi-servo draft. The session itself performs no file or UART access. Its 16 tests were reported passing, with full project verification then passing 949 tests and all 24 checks. Ten subsequent angle-conversion tests were separately confirmed passing.

The owner also confirmed six UART-client hardware checks and a successful interactive save using fictional measurements. See the [console procedure and evidence limits](calibration-console.md). CI for this combined PC-tool revision is pending.
