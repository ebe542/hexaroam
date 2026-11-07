# UART throughput benchmark

## Purpose and scope

The default `sink18` benchmark exercises the COBS/CRC32 command protocol, CommandProcessor, CommandService and a real 18-servo ServoBank with synthetic calibration. The only replaced component is the pulse output: DiscardPulseOutput tracks a simulated active/inactive lifecycle and discards submitted pulses. In `sink18`, no physical servo signals, signal plans, PIO or DMA are generated, and neither EEPROM nor calibration flash is accessed.

After a complete initial pose starts the simulated output, each measured update validates its targets and converts the complete bank into pulse widths. This is a different workload from the earlier register benchmark, which left the bank inactive and only prepared targets. Comparisons therefore include both protocol and workload changes, not COBS overhead alone. This test does not measure a real pending-plan Busy limit or validate a 20-ms physical update deadline.

## Build and connection

From the repository root, with optional hardware dependencies installed in the active Python environment:

```bash
python -m pip install -e ".[hardware]"
python tools/dev.py hardware-build
```

Copy `firmware/pico/target/thumbv6m-none-eabi/debug/examples/uart_benchmark.uf2` to the Pico in BOOTSEL mode. Use this firmware for the default `sink18` fixture. The explicit `--fixture pio2` mode below instead uses the existing two-channel `uart_commands` firmware. The same UF2 supports all three rates.

Use 3.3 V TTL: adapter TX to GP17, adapter RX to GP16, common GND. Remove any UART loopback jumper. Close other programs using the protocol port. The benchmark does not require any physical servo connection.

Set jumpers with power off; selection is sampled once at boot using internal pull-ups:

| GP22 to GND | GP26 to GND | Protocol baud |
| --- | --- | --- |
| Open | Open | 115200 |
| Connected | Open | 128000 |
| Open | Connected | 256000 |
| Connected | Connected | Invalid; panic before protocol startup |

Physical Pico pins: GP22 = 29, GND = 30, GP26 = 31. These are temporary benchmark input assignments. Debug UART TX on GP20 always uses 115200 baud, 8N1. A second TTL receiver can monitor it concurrently. The ready message is `COBS benchmark ready: 18 servos, no physical output`.

## Run and lifecycle

```bash
python tools/uart_benchmark.py --help
python tools/uart_benchmark.py --port COM6 --baud 115200 --batch-size 1 --duration 5
python tools/uart_benchmark.py --port COM6 --baud 115200 --duration 30
```

Default duration is 30 seconds per batch; omitted batch size selects 1, 6 and 18 targets consecutively. `--timeout` defaults to one second per request, including host write time. Match `--baud` to the boot jumpers; supported selections are 115200, 128000 and 256000. The OS and adapter must support the selected rate; the CLI choices alone do not establish hardware support.

Preflight requires Status 1 (valid calibration, disabled, inactive). The protocol has no version/count query: the user must load the correct fixture. The tool then sends OUTPUT_ENABLE(1), a complete 18-angle zero-degree initial pose and GET_STATUS, expecting Status 7. Setup traffic is outside the timed measurements.

Each batch alternates -10 and +10 degrees from Servo 0. The master keeps one request outstanding and checks COBS, response command, ACK/NACK shape and CRC32. In the default sink18 mode, any NACK remains fatal. GET_STATUS must still report 7 after each batch; this check is untimed and is not a target readback or a position measurement. On complete success the tool sends OUTPUT_ENABLE(0) and verifies Status 1.

A timeout, unexpected NACK, invalid response or transport failure aborts without retrying, silently flushing input or issuing follow-up cleanup commands on an uncertain exchange. Only SET_TARGETS NACK(Busy) in the explicit pio2 mode is counted and tolerated. Reset the Pico before another attempt after failure/interruption. After a completely successful run the disabled state permits a new invocation without reset. Changing baud jumpers always requires power-off and restart.

## Timing and diagnostics

There is no 20-ms response settling delay and no per-request console output. Unlimited mode has no artificial inter-request pause. The optional pio2 --rate mode deliberately waits between request starts to enforce its maximum rate. Receive processing reads available data or waits briefly for one byte; the normal serial read timeout is at most 10 ms, shortened near the overall request deadline and then restored. This wait is not a fixed delay after successful replies. Detected unexpected trailing bytes fail; later bytes can also be detected before the next request. Replies arriving after the final observation are not guaranteed to be detected.

The tool reports ACK/NACK counts, requests and targets per second, mean/max successful round-trip time, and measurement-phase timeout/invalid-response/transport-error counts. Round-trip timestamps run from before the PC write call to receipt of the chunk containing the end delimiter; decoding/CRC validation must succeed, but their host processing time is outside that latency timestamp. Overall throughput includes host parsing and validation, USB/adapter/OS overhead and failed-request waiting. Setup, final status checks and final disable are excluded. A final status failure is reported separately from measurement-phase error counts.

The firmware emits cumulative `rx`, `req`, `tx`, `err` and `drop` once after a 500-ms receive pause. Counts include progress returned by successful polls only; a failing poll can consume bytes without returning partial progress. `drop` counts discarded frames/overflow events, not lost bytes. UART errors are logged separately. TX counts mean transport acceptance, not verified wire delivery. Diagnostic messages may be delayed or dropped by the bounded logger. Connect the debug receiver before the run; moving a single adapter afterward may miss the one-shot report.

## Verification and historical result

The owner reported all 59 hardware-build checks passing after the Rust benchmark changes. On 2026-11-07, full verification after the COBS PC-tool migration passed 665 tests and all 17 checks. An initial five-second 115200-baud, one-target run accepted 314 requests in 5.01 seconds without errors (62.6 requests/s; mean/max round trip 15.899/16.366 ms), including successful setup, final status and disable.

Subsequent owner-run measurements at all three baud rates, with the FTDI latency timer still at its original 16-ms setting, completed each batch in 30.01 seconds:

| Baud | Targets per request | ACK | Requests/s | Targets/s | Mean round trip (ms) | Maximum (ms) |
| --- | --- | --- | --- | --- | --- | --- |
| 115200 | 1 | 1880 | 62.6 | 62.6 | 15.862 | 17.099 |
| 115200 | 6 | 1880 | 62.6 | 375.9 | 15.858 | 16.898 |
| 115200 | 18 | 1880 | 62.6 | 1127.5 | 15.856 | 16.803 |
| 128000 | 1 | 1880 | 62.6 | 62.6 | 15.859 | 16.990 |
| 128000 | 6 | 1880 | 62.6 | 375.9 | 15.851 | 16.499 |
| 128000 | 18 | 1880 | 62.6 | 1127.5 | 15.854 | 16.493 |
| 256000 | 1 | 1880 | 62.6 | 62.6 | 15.859 | 17.150 |
| 256000 | 6 | 1880 | 62.6 | 375.9 | 15.847 | 16.817 |
| 256000 | 18 | 1880 | 62.6 | 1127.5 | 15.855 | 16.822 |

All nine batches reported zero NACKs, timeouts, invalid responses and transport errors. All setup, final status checks and final disables passed. Across the nine 30-second batches, 16920 measured target requests were acknowledged over approximately 270 seconds; the earlier 314-request trial and setup/status/disable traffic are additional. A full 18-angle pose was accepted approximately 62.6 times per second in this host-connected, simulated-output fixture.

At the original 16-ms adapter setting, nearly equal latency across packet sizes and baud rates suggested a dominant fixed-latency component. The later adapter-setting comparison below identifies a substantial FTDI/host contribution; neither set of measurements is a standalone Pico throughput limit. The old timeout did not recur, but its original cause is not established.

The later two-channel real-PIO sustained/overload results are recorded below. They do not replace a future full 18-channel output measurement. The owner subsequently confirmed CI for the sink18 benchmark revision; CI for the later pio2 extension is tracked separately below.

### Wire-level timing and adapter investigation

Use two scope channels against common Pico GND: CH1 on GP17 (request into Pico), CH2 on GP16 (reply from Pico). Keep the existing adapter wiring connected. Capture several complete request/reply cycles during a single-batch run; do not disconnect the adapter to attach probes.

Measure these intervals separately:

- Last request stop bit to first reply start bit: firmware-side response turnaround, including polling, validation, encoding and UART launch; not a pure command-handler CPU measurement.
- First to last bit of each packet: actual wire transmission time.
- End of reply to start of the next request: host/adapter/USB path plus host validation and the next send operation.

At 256000 baud, a nominal UART bit is 3.90625 us. Use UART decoding if supported to locate the final zero delimiter and its stop bit; a zero data byte is not an uninterrupted Low frame boundary because UART stop bits return High. The owner supplied captures in the ignored local directory `local-data/measurement`: `delta_tx_rx.bmp` shows approximately 1.244 ms from request end to reply start; `delta_rx_tx.bmp` shows approximately 13.76 ms from reply end to the next request. For one target, the nominal 13-byte request and 9-byte ACK together take 0.859 ms at 256000 baud (8N1). The sum is approximately 15.863 ms, consistent with the host measurement. Cursor placement makes these approximate values rather than exact CPU timing.

`bit_width_256000bd.bmp` shows a 3.6-us single-bit cursor reading at 2.5 MS/s (0.4 us/sample). This is a coarse check, not a precise proof of the nominal 3.90625-us bit period. The successful CRC-checked traffic demonstrates communication under the tested conditions.

Read-only Windows inspection identified COM6 as an FTDI VCP device with LatencyTimer 16 ms. The owner changed the driver setting to 2 ms through Device Manager, without changing firmware or baud rate. See [FTDI latency timer documentation](https://ftdichip.com/Support/Knowledgebase/settingacustomdefaultlaten.htm). This is an adapter/host setting; the benchmark script does not change it. Keep this setting in measurement records and restore the previous value if needed outside these tests.

### Results with FTDI latency timer at 2 ms

An initial one-target, 256000-baud comparison accepted 7516 requests in 30.00 seconds: 250.5 requests/s, mean/max round trip 3.915/4.799 ms, no errors. The owner also observed a scope interval of approximately 2 ms. Later full runs at 256000, 115200 and 128000 baud produced:

| Baud | Targets per request | Duration (s) | ACK | Requests/s | Targets/s | Mean round trip (ms) | Maximum (ms) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 115200 | 1 | 30.00 | 7423 | 247.4 | 247.4 | 3.960 | 5.864 |
| 115200 | 6 | 30.00 | 4486 | 149.5 | 897.1 | 6.588 | 8.480 |
| 115200 | 18 | 30.01 | 2499 | 83.3 | 1499.1 | 11.900 | 13.770 |
| 128000 | 1 | 30.00 | 7471 | 249.0 | 249.0 | 3.933 | 6.052 |
| 128000 | 6 | 30.01 | 4975 | 165.8 | 994.8 | 5.932 | 7.848 |
| 128000 | 18 | 30.01 | 2537 | 84.5 | 1521.7 | 11.735 | 14.269 |
| 256000 | 1 | 30.00 | 7516 | 250.5 | 250.5 | 3.921 | 4.924 |
| 256000 | 6 | 30.00 | 6614 | 220.4 | 1322.6 | 4.452 | 6.737 |
| 256000 | 18 | 30.01 | 3756 | 125.2 | 2253.0 | 7.895 | 9.510 |

All nine batches, setup, final status checks and disables passed, with zero NACKs, timeouts, invalid responses or transport errors. The full runs acknowledged 17886 measured requests at 256000 baud, 14408 at 115200 baud and 14983 at 128000 baud: 47277 measured requests over approximately 270 seconds. The earlier 7516-request one-target comparison is separate.

The one-target throughput increased approximately fourfold after reducing the adapter latency setting. Packet-size effects are now visible. Complete 18-angle poses were accepted at approximately 125.2/s, but this does not demonstrate the rate of physical PIO plan application. No measurement-phase code changes were made for the latency-setting comparison. With the same 2-ms adapter setting, 115200 baud accepted 83.3 complete 18-angle poses/s (mean/max round trip 11.900/13.770 ms), compared with 125.2/s at 256000 baud (7.895/9.510 ms). The measured full-pose throughput gain is approximately 50%; one-target rates remain nearly equal. At 128000 baud, full-pose throughput was 84.5/s (mean/max round trip 11.735/14.269 ms), only about 1.4% above the 115200-baud result in these runs. No default operating baud rate is selected by these measurements alone; the later real-PIO test must account for signal-cycle acceptance and Busy. The earlier 16-ms measurements must not be compared as if their host configuration were identical. Measured average throughput and observed maxima are not worst-case timing guarantees for real PIO output.

Historical register-protocol result: at 115200 baud, batch size 1, 67 requests succeeded before a response timeout. Successful mean/max round trips were 15.947/16.898 ms; elapsed time was 2.08 seconds including the failed wait. The cause remains unresolved. Earlier build success and the separately passed COBS functional UART suites do not establish that this sustained-load issue is fixed.

## Sustained real PIO/DMA output: explicit pio2 fixture

This mode reuses `uart_commands.uf2`, fixed at 115200 baud, two synthetic calibrations and actual PIO/DMA on unloaded GP0/GP1. There is no new firmware copy or firmware behavior change. Flash that fixture instead of uart_benchmark before selecting `--fixture pio2`. No servos or mechanical loads may be connected. The baud-selection jumpers of uart_benchmark have no role in uart_commands. Keep the FTDI latency setting at 2 ms for comparable host conditions.

The tool sends Enable and a complete two-angle zero-degree initial pose. Batch 2 then alternates GP0/GP1 between 1000/2000 us and 2000/1000 us. Batch 1 alternates only GP0; GP1 starts and remains at 1500 us. The default batch selection for pio2 is 1 and 2; use --batch-size 2 for the prescribed first run. sink18 remains the default fixture with batches 1, 6 and 18.

First run with at most 25 requests/s for 60 seconds:

```bash
python tools/uart_benchmark.py --port COM6 --fixture pio2 --baud 115200 --batch-size 2 --rate 25 --duration 60
```

Observe both channels with common ground. Check 20-ms periods, synchronous rising edges, pulse widths restricted to the expected values (apart from the 1500-us initial pose), absence of unexplained missing or malformed pulses, and Low outputs after final disable. A stopping pulse may be shortened by explicit disable, as allowed by the driver contract. Start timestamps are spaced by at least the requested interval, with host scheduling allowed to make them later. Whole missed pattern slots are skipped, never sent in a catch-up burst. This is not a hard real-time 25-Hz guarantee.

After the paced run is validated, omit --rate for overload:

```bash
python tools/uart_benchmark.py --port COM6 --fixture pio2 --baud 115200 --batch-size 2 --duration 60
```

Only a CRC-valid NACK for SET_TARGETS with code 0x05 is counted as expected Busy. Other NACKs, framing/CRC errors, receive timeouts or transport failures abort. A Busy response means that request was rejected; it is not queued or automatically retried. Unlimited mode advances the alternating pattern on every attempt, not only on ACK. Paced mode selects the pattern from elapsed time. Due to skipped/rejected patterns, accepted physical pulses need not alternate on every cycle. Report attempted requests, ACK, Busy/other NACK, Busy percentage, skipped schedule slots and ACK-only latency. A run with no accepted updates fails even if it received only Busy.

ACK throughput counts accepted commands, not separately measured frame-boundary applications. At most one replacement plan may remain pending; the last accepted plan can still be pending when the test sends Disable. No inference of measured servo positions is possible. The scope and existing software ownership tests complement each other; this visual two-value pattern alone is not proof against every possible stale-plan defect.

Final Status 7 is checked outside the measurement, then successful completion sends Disable and verifies Status 1. After any failed or interrupted run, reset the Pico; follow-up cleanup is not attempted on an uncertain exchange. Failed tests may leave physical output active until reset. No running error is cleared automatically.

### pio2 verification

The owner reported full project verification passing 665 tests and all 17 checks after pio2 support was added. The first real-PIO paced run used 115200 baud, batch size 2, maximum rate 25/s and 60.00 seconds:

- 1486 attempts, all 1486 acknowledged; no Busy or other NACK.
- 24.8 accepted requests/s and 49.5 targets/s.
- ACK round-trip mean 5.896 ms, observed maximum 7.445 ms.
- Zero timeouts, invalid responses or transport errors.
- 14 skipped host schedule slots; these are unsent timing opportunities, not lost serial packets.
- Setup, final running-status check and final disable/status check all passed.
- The owner confirmed observed pulse widths of 1 and 2 ms. Period, rising-edge synchronization and final physical Low were not individually reported with this paced result; the later overload result included a general confirmation that pulses were correct. The earlier functional scope suite separately confirmed its documented signal behavior.

The owner then completed a 60.00-second unlimited run with the same firmware, baud rate, batch size and adapter setting (the request-rate limit alone was removed):

| Quantity | Observed |
| --- | --- |
| Attempts | 10021 |
| ACK | 3000 |
| NACK(Busy) | 7021 (70.1%) |
| Other NACK | 0 |
| Accepted requests/s | 50.0 |
| Accepted targets/s | 100.0 |
| ACK round-trip mean / maximum | 5.861 / 6.742 ms |
| Timeout / invalid response / transport errors | 0 / 0 / 0 |
| Skipped schedule slots | 0 (unlimited mode) |

Setup, final running status and final disable/status all passed. The owner confirmed that the pulses were correct during this run; no additional numerical scope capture was supplied. Busy is expected backpressure here, not a technical fault or a counted accepted target update. The observed 50.0 accepted requests/s is consistent with one replacement-plan acceptance per configured 20-ms signal cycle. ACK counts are not independently counted physical applications, and this test covers two channels rather than all 18.

The paced run and overload run are both owner-verified on hardware, with the software baseline passing 665 tests and all 17 checks. CI for this pio2 extension remains pending. No new firmware image or hardware build was needed for the Python-only pio2 extension.
