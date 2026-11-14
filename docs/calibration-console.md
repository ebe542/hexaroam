# Calibration console

`hexaroam-calibrate` is the interactive PC tool for a single-servo calibration station. It sends pulse and output commands over UART0 and records manually measured angles into a JSON document. It does not read angle feedback or program an EEPROM.

## Installation

Use Python 3.10 or newer and the intended virtual environment. From the repository root:

```bash
python -m pip install -e ".[hardware]"
```

The distribution is named hexaroam-test-tools. Its regular Python package is [hexaroam_tools](../tools/hexaroam_tools/__init__.py), discovered under tools/ by pyproject.toml. Installation registers the `hexaroam-calibrate` console command. An editable installation uses the working-tree sources; reinstall after changing entry points or package metadata.

Select the same environment in the editor and terminal. No extraPaths setting or runtime sys.path modification is required for the package or its manual hardware check. The CI jobs install the local package without hardware extras before running tests; pySerial is loaded only when opening a real connection.

## Firmware and connection

Upload the normal [calibration-station application](../firmware/apps/calibration-station/README.md). UART0 uses GP16 TX, GP17 RX, 115200 baud, 8N1 and no flow control. Connect a 3.3 V TTL adapter with crossed TX/RX and common GND. Adapter power outputs remain disconnected. UART1 debug output is separate on GP20.

The current Windows setup uses COM8 for UART0 protocol and COM7 for UART1 debug. Close other clients of COM8 before opening the console. On Ubuntu substitute the appropriate port, for example /dev/ttyUSB0. The initial console test used unloaded GP0; no physical servo or mechanical calibration has been verified yet.

## Start a session

Run from the repository root:

```bash
hexaroam-calibrate --port COM8 --file local-data/calibration/servos.json --servo-count 18 --servo-id 0
```

For an existing document, --servo-count can be omitted. If supplied, it must match the loaded count. A new document requires --servo-count. The selected ID must lie in 0 through servo_count - 1. Invalid documents or IDs are rejected before the port is opened. A new draft is not written until save succeeds.

The selected ID identifies only the JSON entry. The station always drives its single physical output, GP0; the ID does not select another GPIO. Choose the correct logical ID for the servo being measured and label the physical servo accordingly.

The program loads the document, creates a fresh measurement session, connects and explicitly disables output. A prepared pulse may remain from earlier use, and status displays it. Connection does not automatically start output. Run another invocation with a different --servo-id to capture another servo; existing entries are retained.

## Commands

| Command | Effect |
| --- | --- |
| status | Read flags, last accepted pulse, running state and output fault |
| pulse 1500 | Request pulse width in microseconds; report success only after ACK |
| enable | Start the prepared pulse |
| disable | Stop output and retain the pulse |
| lower -45.00 | Record a measured lower angle at the station's current accepted pulse |
| upper 45.00 | Record a measured upper angle at the station's current accepted pulse |
| save | Validate both points, stop and confirm inactive output, then write the document |
| help | Show commands |
| quit | Leave the console and attempt to stop output before closing the port |

Angles accept point or comma as decimal separator and at most two fractional digits. Examples: -45, -45.25 and -45,25. Enter a leading zero for fractional values such as 0.50. Extra precision and scientific notation are rejected rather than rounded. Internal values are signed integer centidegrees.

To capture a point, set the pulse, enable output, wait for the physical position to settle, measure the angle against the chosen bench zero and enter lower or upper. Capture requires prepared, running output without an output fault. The console queries status for the accepted pulse; a rejected pulse request cannot become a measurement's pulse value. Status describes accepted targets, not a sensor-measured position or proof that a pending pulse update has reached the pin.

Both points must be valid before save. Invalid input, invalid pairs and NACKs leave previous captured points available. Saving adds or replaces just the selected servo in the loaded document and retains other entries. Global completeness is checked separately before future export/programming. If a file write fails, the console retains the captured points for another save attempt. A successful save leaves output disabled.

quit asks before discarding unsaved captured points. EOF or interruption does not save automatically. Normal exit, EOF and interruption attempt an explicit output stop while the connection is usable. Merely closing a serial port does not stop PIO output. If communication fails and stop cannot be confirmed, the program reports that the Pico must be reset.

## Transport behavior

[wire_protocol.py](../tools/hexaroam_tools/wire_protocol.py) contains shared CRC32/COBS encoding, reception and command exchange used by the client, UART suite runner and benchmark. It does not depend on pySerial. [calibration_client.py](../tools/hexaroam_tools/calibration_client.py) opens UART and exposes get_status, set_pulse and set_enabled.

Commands are synchronous, one outstanding request at a time, without automatic retries. A valid NACK includes the original command and error code and leaves the connection usable. An I/O timeout or malformed response closes and invalidates the client, so subsequent requests cannot mistake a delayed response for their own. The firmware continues to use its existing nonblocking output-service loop.

## Repeatable client hardware check

With the normal firmware and GP0 unloaded:

```bash
python firmware/apps/calibration-station/hardware_tests/check_client.py --port COM8
```

The check explicitly disables output, prepares 1500 us, enables, updates to 1700 us, verifies a 2001 us InvalidValue NACK preserves the pulse and connection, and stops while retaining 1700 us. It reports six PASS messages. It is manual hardware code, not part of unittest discovery or CI. It does not require a Pico restart unless a fault or failed connection prevents normal operation.

## Console smoke check

Use a separate file for fictional values, with GP0 unloaded:

```bash
hexaroam-calibrate --port COM8 --file local-data/calibration/console-check.json --servo-count 1 --servo-id 0
```

Enter:

```text
pulse 1500
enable
lower -45.00
pulse 1700
pulse 2001
upper 45.00
save
quit
```

The 2001 us request must be rejected. The saved entry contains lower angle -4500 at 1500 us and upper angle 4500 at 1700 us. These are synthetic software-check values, not physical calibration measurements. The personal file remains under ignored local-data/.

The owner confirmed the console check succeeded, and the resulting synthetic JSON was inspected with those exact values. The preceding client hardware check passed all six operations and full project verification reported 949 tests and all 24 checks. The subsequent ten angle-conversion tests were separately confirmed passing; a later console installation/verification procedure was reported successful without a numeric total. No simulated serial port was used. Physical servo measurements, binary export and EEPROM programming remain pending. CI for this combined PC-tool revision is pending.
