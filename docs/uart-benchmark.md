# UART throughput benchmark

## Purpose and scope

The benchmark exercises the existing version-2 register protocol, real frame processing and an 18-servo ServoBank with synthetic calibration. It does not implement the proposed COBS command protocol. Retain it as a baseline; adapting it to a future protocol is separate work.

The bank stays inactive and accepts prepared targets. This measures validation and target storage, not active pulse-plan generation, PIO or DMA. No physical servo output is generated and no calibration flash is accessed. The discard output is test-only; its lifecycle implementation does not activate output.

## Build and connection

From the repository root, with the hardware Python dependencies installed in the active environment:

```bash
python -m pip install -e ".[hardware]"
python tools/dev.py hardware-build
```

Copy `firmware/pico/target/thumbv6m-none-eabi/debug/examples/uart_benchmark.uf2` to the Pico in BOOTSEL mode. The same firmware supports all three rates.

Use 3.3 V TTL: adapter TX to GP17, adapter RX to GP16, common GND. Remove any UART loopback jumper. Close other programs using the protocol port.

Set jumpers with power off; selection is sampled once at boot using internal pull-ups:

| GP22 to GND | GP26 to GND | Protocol baud |
| --- | --- | --- |
| Open | Open | 115200 |
| Connected | Open | 128000 |
| Open | Connected | 256000 |
| Connected | Connected | Invalid; panic before protocol startup |

Physical Pico pins: GP22 = 29, GND = 30, GP26 = 31. These are temporary benchmark input assignments. Debug UART TX on GP20 always uses 115200 baud, 8N1. A second TTL receiver can monitor it concurrently.

## Run

```bash
python tools/uart_benchmark.py --port COM6 --baud 115200 --duration 5
python tools/uart_benchmark.py --port COM6 --baud 115200 --batch-size 1 --duration 5
```

Default duration is 30 seconds per batch; omitted batch size selects 1, 6 and 18 targets consecutively. `--timeout` defaults to one second per request. Match `--baud` to the boot jumpers; supported selections are 115200, 128000 and 256000.

Preflight requires protocol version 2, Status 1 (valid calibration, inactive output) and ServoCount 18. Each batch alternates -10 and +10 degrees from Servo 0. The master keeps one request outstanding, validates complete ACK/NACK frames and CRC32, and stops on error without retrying. Final target readback occurs outside the measured interval.

The tool reports ACK/NACK counts, requests and targets per second, mean/max successful round-trip time, and timeout/invalid-response counts. Round-trip timing starts before the PC write call and ends on receipt of the complete response; it includes host/USB/adapter effects. Failed-run throughput includes time spent waiting for the failed request, while mean/max latency cover successful requests only. A serial exception or readback timeout can abort before the batch summary.

The diagnostic firmware emits cumulative `Poll totals` once after a 500-ms receive pause. Counters include progress returned by successful polls only; a failing poll can consume bytes without reporting its partial progress. UART errors are logged separately. TX counts mean transport acceptance, not verified wire delivery. Diagnostic messages may be delayed or dropped by the bounded logger. Keep the debug receiver connected before the run; the one-shot report may be missed when moving a single adapter afterward.

## Verification and unresolved result

The owner reported all 55 hardware-build checks passing after diagnostic integration. An earlier hardware run at 115200 baud, batch size 1, accepted 67 requests and then timed out waiting for the next response header. Successful mean/max round trips were 15.947/16.898 ms; elapsed time was 2.08 seconds including the failed wait. The cause is unresolved. No successful diagnostic rerun, higher-rate result or active-PIO throughput result has been reported. Build success is not a passed throughput test.
