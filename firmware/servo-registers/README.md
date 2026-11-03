# Servo register service

Hardware-independent no_std register handling above [servo-protocol](../servo-protocol/README.md). It consumes ParsedFrame and produces a READ-response Frame borrowing a caller-provided payload buffer. The frame codec adds wire framing/CRC; this package does not use UART, Pico registers, flash or the real PIO/DMA output driver.

## Implemented register subset

| Index | Register | Bytes | Access |
| --- | --- | ---: | --- |
| 0x00 | Protocol version | 2 | READ |
| 0x01 | Control/backend status | 1 | READ |
| 0x02 | LastError | 4 | READ; WRITE zero to clear |
| 0x03 | Control | 1 | READ, WRITE, SET, CLR |
| 0x04 | Servo count | 1 | READ |
| 0x20..0x31 | Target angles for servo IDs 0..17 | 4 per servo | READ, WRITE |

The map is exported as REGISTER_REGIONS for parser configuration. Index/access/length checks are repeated at the service boundary; payload length is checked before indexed access or side effects. READ failures return the normal LastError register. WRITE/SET/CLR produce no response. Successful operations do not clear a previous LastError. A too-small local response buffer leaves registers and buffer unchanged; ServiceError represents that caller-side failure rather than a protocol error.

Error codes currently are None=0, UnknownIndex=1, UnsupportedAccess=2, InvalidLength=3, OutOfBounds=4, InvalidValue=5, NotConfigured=6, InvalidState=7, OutputError=8. They describe this initial implementation, not a separately frozen production protocol.

## Control and output contract

ControlRegister owns an OutputBackend. Bit 0 of Control is OUTPUT_ENABLE; other bits are reserved and rejected even for CLR. Status currently reports CALIBRATION_VALID at bit 0, OUTPUT_ACTIVE at bit 1 and a latched FAULT at bit 3. BUSY is not implemented. Calibration validity is supplied at construction; this does not load or independently validate actual calibration data.

WRITE replaces Control, SET applies OR and CLR clears selected bits. The complete request is checked before a backend call. Enable requires calibration and no fault. Backend start validates the prepared initial targets; InvalidTargets rejects the request without a technical fault. Technical start/stop errors latch Fault and preserve the previously accepted Control value. Status reads OutputBackend::state, including after an error, so it may differ from the requested enable state. Repeated enable/disable avoids redundant backend start/stop calls. Disabling remains possible in Fault, but does not clear Fault. Clearing LastError also does not clear Fault.

OutputBackend provides start, stop, a reliable logical Active/Inactive state, target reads and transactional target writes. It does not report mechanical position or servo power. The concrete future adapter must uphold the post-error state contract. The package provides both a shared simulated backend and a direct OutputBackend implementation for ServoBank, described below. Hardware integration of the register-service path remains pending.

## Modules and tests

- [service.rs](src/service.rs): fixed register dispatch, responses and LastError.
- [control.rs](src/control.rs): enable transitions and control-related status.
- [output.rs](src/output.rs): output backend contract and typed start failures.
- [test_support.rs](src/test_support.rs): shared backend for host tests only.
- [test_pipeline.rs](src/test_pipeline.rs): byte-stream parser, service and response-codec integration tests.
- [test_targets.rs](src/test_targets.rs): target reads/writes, validation, state guards and technical failures.

From the repository root:

```bash
cargo test --manifest-path firmware/servo-registers/Cargo.toml --lib --locked
python tools/dev.py fmt
python tools/dev.py verify
python tools/dev.py doc
```

On 2026-10-31 the owner reported 41 passing host tests after the intended failing phases: 14 original service tests, 16 control tests and 11 service/control integration tests. Coverage includes canonical error responses, no reply/output-buffer writes for modifying requests, error retention/explicit clearing, reserved bits, missing calibration, invalid initial targets, actual output state after failures and fault persistence. The owner subsequently confirmed all 17 full verification checks and all 9 shared documentation checks passing. The owner confirmed green CI for this register-service revision. No tests were executed by the assistant.

The calibration region, invalidation, Busy/asynchronous status events, flash integration, UART delivery and the firmware loop remain future work. The direct ServoBank output integration is described below. This package is not included by the normal Pico firmware yet.

## Byte-stream integration tests

Seven host tests connect request encoding, incremental parsing, register handling and response encoding/decoding. They cover status reads, SET without a direct response followed by status readback, canonical LastError responses, CRC-corrupted requests without service side effects, recovery after a corrupted frame, timeout recovery and consecutive SET/CLR/READ requests. Recovery and consecutive-request tests retain the same parser instance. Timeout is an explicit event; these tests do not measure UART timing.

On 2026-10-31 the owner reported all 48 package tests and 375 tests overall passing, with all 17 verification checks passing. The owner subsequently confirmed green CI for this integration-test revision. The assistant did not execute tests.

## Target-angle registers

Each target occupies one logical index and carries an i32 little-endian value in hundredths of a degree. A request may access one or several consecutive targets. The current map supports up to eighteen servos; the service additionally checks the configured servo count. SET/CLR are rejected for target registers. READ reports accepted targets from the backend, not measured positions or a second register-service copy.

ControlRegister rejects reads and writes without calibration and rejects writes while Fault is latched. Reads remain available in Fault when calibration is valid. Inactive writes prepare targets without enabling output; active writes submit a batch through OutputBackend::set_targets. The backend must validate the entire batch and preserve all accepted targets on failure. Success confirms acceptance, not that a new signal plan is already active or a servo has reached its position. Technical write errors latch Fault; invalid angles do not. Successful accesses preserve LastError.

The register service decodes the full batch before one backend call and stages complete READ data before touching the response buffer. TargetReadError and TargetWriteError keep backend errors separate from register error codes. Initial targets are supplied by the backend; the shared test backend initializes them to zero and uses a synthetic -90 to +90 degree range. Journal-loaded calibration and register-service-to-PIO hardware integration remain pending. The direct ServoBank backend below uses the bank's configured calibrations.

On 2026-10-31 the owner observed the intended failing phase (48 passed, 8 failed), then confirmed 56 package tests passing. The complete verification reported 383 passing tests and all 17 checks passing. The eight new tests cover backend-supplied initial values, inactive preparation, active writes, atomic rejection of invalid angles, malformed requests and bounds, missing calibration, fault latching and short READ buffers. The owner subsequently confirmed green CI for this target-register revision. No tests were executed by the assistant.

## Target-register byte-stream tests

Four additional tests in [test_pipeline.rs](src/test_pipeline.rs) connect target requests to CRC-checked response bytes using the same parser across requests. They cover all eighteen targets (72 payload bytes) with inactive and active output, partial writes preserving neighboring targets, complete rejection of a batch whose final angle is invalid, and canonical errors for a READ crossing the target region followed by a valid READ. Values include negative angles and nonzero centidegrees. Modifying requests produce no response, and successful readback preserves LastError. Test response-payload buffers now hold all eighteen target values.

The owner reported 60 package tests and 387 tests overall passing on 2026-10-31, with all 17 verification checks passing. CI for this target-wire-test revision is pending. These tests use the simulated backend; actual servo signals, UART timing and ServoBank integration are not exercised. No tests were executed by the assistant.

## Direct ServoBank backend

[servo_bank_backend.rs](src/servo_bank_backend.rs) implements OutputBackend directly for ServoBank<O, N> when O implements PulseBankLifecycle. There is no extra wrapper object, target store or cached output state. Startup delegates to start_prepared; missing/invalid targets become InvalidTargets, while lifecycle errors retain their type through ServoBankBackendError::Lifecycle. Stop delegates without changing stored targets. State translates the backend's confirmed logical state.

Target reads distinguish unknown and uninitialized servos. Uninitialized reads/writes map to register InvalidState without latching a technical Fault. Inactive writes call prepare_targets; active writes call set_targets. Domain validation errors retain their meaning, and submission errors are preserved through ServoBankBackendError::Submission. Batch rejection preserves all accepted targets.

RegisterService::service forwards through ControlRegister and OutputBackend to ServoBank::service. The application must call it regularly, independently of incoming register requests, so DMA/PIO servicing remains reachable while the service owns the bank. This is servicing only; no asynchronous backend-fault reporting or automatic recovery policy is introduced. Calibration validity remains supplied at construction and automatic zero-degree initialization is not added.

[test_servo_bank_backend.rs](src/test_servo_bank_backend.rs) uses a real two-servo bank with individual synthetic calibrations and a recording pulse/lifecycle backend. Twelve tests cover missing/unknown targets, inactive partial preparation, calibrated startup, incomplete startup, active updates, atomic rejection, incomplete active writes, lifecycle/submission errors and state, stop retry/restart, register-service operations and service forwarding. Register READ responses are checked directly; UART transport is not exercised.

On 2026-11-01 the owner observed the intended red phase (60 passed, 12 failed), then confirmed all 72 package tests passing. Full verification reported 427 tests and all 17 checks passing. No tests were executed by the assistant. The register-service-to-real-PIO hardware path and UART remain unverified; the preceding ServoBank lifecycle hardware test is a separate result. CI for this backend-integration revision is pending.

## Frame processing over a byte transport

[FrameProcessor](src/frame_processor.rs) owns RegisterService, a request StreamParser, one ResponseBuffer<MAX_FRAME_SIZE> and a pending receive-timeout flag. Each poll first services the output backend once, then transmits an existing response up to its byte budget. While response bytes remain pending, reception and further request processing stop. Newly created responses are encoded and copied into the response buffer, then transmitted on a later poll. WRITE/SET/CLR have no direct reply. The master must wait for its READ response before sending more requests.

When response space is available, poll drains complete buffered requests before reading another transport byte. A pending timeout clears only the incomplete remainder after complete requests have been drained, even if backpressure defers this across multiple calls. The caller must detect the timeout externally and account for unread transport bytes. No clock, UART idle detection or timeout duration is implemented here. Receive errors are returned without implicitly clearing the parser; transmit errors preserve the unsent suffix. Byte budgets bound transport calls rather than elapsed processing time or register-operation cost.

FrameProgress reports consumed receive bytes, accepted transmit bytes, processed requests and whether a receive attempt found no input. False receive_empty also covers a call that did not attempt reception. Transport acceptance does not establish physical wire completion. FrameProcessingError distinguishes receive/transmit, parser, register-service, encoding and response-queue failures.

Twelve tests in [test_frame_processor.rs](src/test_frame_processor.rs) cover idle/zero-budget output servicing, fragmented requests, request order, response backpressure, transport errors, CRC resynchronization, timeout recovery, preservation of complete buffered requests during a deferred timeout and eighteen-target responses with small budgets. These tests use the shared simulated register backend, not the recording ServoBank backend from the separate bank-integration tests. No single byte-transport-to-real-PIO hardware test is claimed.

On 2026-11-02 the owner observed the intended red phase (72 passed, 12 failed), then confirmed all 84 register-package tests passing. Combined with the response-buffer work, full verification reported 451 tests and all 17 checks passing. The concrete Pico UART transport, physical timing, error-recovery policy and normal firmware loop remain future work. No tests were executed by the assistant. CI for this combined revision is pending.

## Physical UART register reads

On 2026-11-02 the owner confirmed all three exact responses from [uart_registers](../pico/hardware_tests/README.md#uart-register-requestresponse-test) in HTerm: Version=1, Status=0 and ServoCount=0. The example connects the concrete UART transport and FrameProcessor to RegisterService with NoServoOutput. It does not use the real ServoBank/PIO backend, journal-loaded calibration or the normal application. Full verification reported 451 tests and all 17 checks passing. The example has a provisional 100 ms receive-idle policy, whose recovery behavior was not exercised in these three READs. CI for this hardware-example revision is pending.
