# UART calibration persistence hardware test

This manual test uses [uart_calibration_persistence.rs](../firmware/apps/servo-controller/hardware_tests/uart_calibration_persistence.rs) and its [JSON suite](../firmware/apps/servo-controller/hardware_tests/suites/uart_calibration_persistence.json). It connects real UART requests, ACK/NACK, the RAM calibration request, response-buffer readiness and the reserved Pico flash journal. NoServoOutput keeps physical servo outputs inactive; no core1 or DMA is started.

## Build and connection

The owner reported all **53 hardware-build checks passing** on 2026-11-04. On 2026-11-04 the owner subsequently confirmed Fixture check and all three UART scenarios passing after restart, using COM6 and a 5000-ms response timeout. The owner subsequently confirmed B after restart at sequence 28, slot 11; see the restart result below. Host persistence tests are separate evidence (515 total host tests and all 17 checks passed before this hardware test).

```bash
python tools/dev.py hardware-build
```

Flash `firmware/apps/servo-controller/target/thumbv6m-none-eabi/debug/examples/uart_calibration_persistence.uf2` using BOOTSEL. The firmware appends synthetic records to the reserved calibration journal and may rotate/erase a sector under the normal journal policy; it does not erase the whole journal as preparation. Existing journal data must therefore be test data appropriate to this experiment.

Connect the 3.3-V TTL adapter: Pico GP16/TX -> adapter RXD, GP17/RX <- adapter TXD, common GND. Remove the GP16-GP17 loopback jumper. Protocol uses 115200 baud, 8N1, no flow control; diagnostic UART1 TX remains GP20. Close HTerm or any other application using the protocol port.

## Run the three cases

Reset the Pico before every suite invocation. The startup preflight expects version 2, Status=0 and ServoCount=0. Loading the journal in this test only checks/logs existing data; it does not install active calibration or change ServoCount. After persistence the result remains in Status until another accepted request or reset.

From the repository root with the optional hardware dependencies installed:

```bash
python tools/uart_test.py --port COM6 --suite uart_calibration_persistence
```

Replace COM6 with your actual device, for example `/dev/ttyUSB0`. Run the whole suite in order. Selecting only `repeat_a` with --case does not establish that A was already persisted by the preceding case and therefore is not a complete duplicate-request test.

| Case | Operation | Required outcome |
| --- | --- | --- |
| store_a | Complete calibration WRITE of A, then poll Status | Empty ACK for index 10; eventual Status=00000100 |
| repeat_a | Send exactly the same A again | Success; firmware asserts no additional write/erase and unchanged slot/sequence |
| store_b | Send changed B | Success; firmware asserts exactly one additional write and sequence increment |

The first A may already equal a previous valid record; either changed or unchanged storage is allowed for that setup step. The second A must be unchanged after the first case succeeds. B differs from A and must advance the sequence. The firmware verifies the complete decoded stored calibration, not only its CRC. Control and ServoCount remain zero throughout.

Status success is u32 0x00000100 (little-endian payload `00 01 00 00`). Busy is 0x00000004 (`04 00 00 00`). Every reply step, including each poll, has a 5000-ms response timeout and the usual 20-ms settling window. Only the exact Busy reply permits another poll, with a 20-ms delay and at most 50 attempts. Missing, corrupt, extra, unexpected or failure-status bytes fail immediately; timeout is not automatically retried. The calibration WRITE itself is never automatically repeated by the runner.

## Synthetic datasets

Both serialized datasets contain eighteen entries, the existing four-byte header and inner CRC32: 368 payload bytes and a 376-byte WRITE including the protocol header/outer CRC32.

For ServoId i from 0 through 17:

| Field | Dataset A | Dataset B |
| --- | --- | --- |
| lower angle, centidegrees | -4500 + 10*i | -4500 + 10*i |
| upper angle, centidegrees | 4500 + 10*i | 4500 + 10*i |
| lower pulse, microseconds | 1000 + 5*i | 1100 + 5*i |
| upper pulse, microseconds | 2000 - 5*i | 2100 - 5*i |

All exact bytes and CRCs are in the JSON suite. These values are synthetic storage fixtures, not measured calibration for a robot.

## Firmware assertions and restart check

UART1 emits one boot record line and one result line per persistence operation, without periodic messages:

```text
INFO Boot journal: seq=... slot=... fixture=...
INFO UART calibration persistence ready
INFO Persist OK: seq=... fixture=A unchanged=... writes=... erases=...
INFO Persist OK: seq=... fixture=A unchanged=true writes=... erases=...
INFO Persist OK: seq=... fixture=B unchanged=false writes=... erases=...
```

Numbers depend on the journal's previous contents. On the duplicate A, sequence/slot and both backend counters must equal the preceding A result. B advances the sequence by one. A comparison failure or storage error panics through UART1 and halts the test; the firmware sets the LED high on its persistence-check error path. During normal operation the LED blinks. The counters count attempted backend calls and reset at boot.

For the separate persistence check, stop the Python runner and reset or power-cycle the Pico while monitoring UART1 (GP20, 115200/8N1). The boot line must report `fixture=B`. If the diagnostic stream was captured during the suite, compare its final sequence and slot with the boot line; both must match. Capturing UART0 protocol and UART1 diagnostics simultaneously requires independent receive connections. With only the protocol adapter moved to UART1 after the suite, the boot check confirms B and reports its sequence, but does not by itself compare that sequence against an uncaptured pre-reset log. Record these observations separately from the script's three-case summary.

The test does not use a protocol restart command, detect ACK loss directly, activate servos or establish worst-case flash/UART latency. It does verify no duplicate write through the firmware's backend counters when the suite reaches and completes its repeated-A case.

## Receive and timing diagnostics (2026-11-04)

The owner observed UART RX overrun on the initial 376-byte request. After retaining the validated frame candidate instead of repeating header/map validation for every payload byte, all 87 protocol tests passed and one hardware request persisted A at sequence 27 with one write and no erase. This confirms that request, not arbitrary sustained UART throughput.

A COM6 diagnostic then measured the correct ACK after 0.047 s and the correct Success Status after 1.880 s; the LED continued blinking. The original 1000-ms response timeout was too short for that blocking debug-test path. All response steps in this calibration suite now allow 5000 ms, while the ordinary register suite retains 1000 ms. Correct complete replies still finish after the 20-ms settling window, without waiting the entire timeout. Before/after journal checks and software CRC work add test overhead; Busy does not make the synchronous storage call answer UART requests concurrently. These observed times are not worst-case bounds. The owner subsequently confirmed all three cases passing with the revised timeout after restarting the Pico. A subsequent restart reported fixture=B at sequence 28, slot 11.

## Three-case hardware result (2026-11-04)

The owner reported Fixture check, store_a, repeat_a and store_b passing after restart. The repeated-A path therefore completed the firmware assertions that journal data, slot/sequence and write/erase counters remained unchanged; changed B completed the verified new-record path. Before restarting, preflight rejected Status=0x00000100 (retained Success) rather than startup Status=0. This is the documented fixture-state requirement, not a protocol failure. The owner subsequently reported B remaining readable after restart at sequence 28, slot 11. The owner subsequently reported full local verification passing 520 tests and all 17 checks; CI for the combined revision has not yet been reported.

## Restart result (2026-11-04)

After the three-case suite the owner reported:

```text
INFO Boot journal: seq=28 slot=11 fixture=B
INFO UART calibration persistence ready
```

This confirms that the newest CRC-valid record is readable after restart and exactly matches synthetic dataset B. The earlier isolated A transfer reported sequence 27; sequence 28 is consistent with the changed B write and the firmware's duplicate-A assertions. An explicit pre-reset B sequence/slot log was not supplied, so no additional direct before/after comparison of that log is claimed. Full local verification of the final changes passed 520 tests and all 17 checks according to the owner. CI for this combined revision has not yet been reported.
