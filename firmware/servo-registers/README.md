# Servo register service

Hardware-independent no_std register handling above [servo-protocol](../servo-protocol/README.md). RegisterService consumes CRC-checked ParsedFrame values and returns ACK/NACK frames borrowing caller-provided payload storage. The frame codec adds header/CRC bytes; this package does not directly access UART hardware or flash. See the [version-2 contract](../../docs/servo-register-protocol.md).

## Implemented register subset

| Index | Register | Bytes | Access |
| --- | --- | ---: | --- |
| 0x00 | Protocol version; UART fixture reports 2 | 2 | READ |
| 0x01 | Control/backend status | 1 | READ |
| 0x02 | Unassigned; reserved for future restart reason | — | None |
| 0x03 | Control | 1 | READ, WRITE, SET, CLR |
| 0x04 | Servo count | 1 | READ |
| 0x20..0x31 | Target angles for servo IDs 0..17 | 4 per servo | READ, WRITE |

REGISTER_REGIONS supplies parser configuration. The service independently revalidates access and payload size before indexed reads or side effects. Successful READ returns ACK with data; successful WRITE/SET/CLR returns an empty ACK. Errors return NACK with the original index and one error byte. LastError was removed. The current handle signature retains Option<Frame>, but every handled request produces a response unless a local ServiceError occurs.

The stream parser discards unknown indices and implausible lengths before dispatch. Direct service callers can still receive UnknownIndex/InvalidLength NACKs. ErrorCode is repr(u8): None=0, UnknownIndex=1, UnsupportedAccess=2, InvalidLength=3, OutOfBounds=4, InvalidValue=5, NotConfigured=6, InvalidState=7, OutputError=8. Local response-capacity errors leave state and caller storage unchanged; modifying operations reserve capacity for a possible one-byte NACK before executing.

## Control and output contract

ControlRegister owns an OutputBackend. Control bit 0 is OUTPUT_ENABLE; reserved bits are rejected even for CLR. WRITE replaces Control, SET applies OR and CLR clears selected bits. Enable requires calibration and no Fault. Backend InvalidTargets rejects startup without a technical fault; technical start/stop errors latch Fault and preserve accepted Control values. Repeated enable/disable avoids redundant calls. Disable remains possible in Fault but does not clear it.

Status reports CALIBRATION_VALID at bit 0, actual OUTPUT_ACTIVE at bit 1 and latched FAULT at bit 3. It reports logical backend state, not measured position or servo power. Calibration validity is supplied at construction. Busy, action results, restart reason, restart and calibration transfer are not implemented.

## Target-angle registers

Each target occupies one logical index and carries signed i32 little-endian centidegrees. Requests may span consecutive targets within the static region and configured servo count. SET/CLR are prohibited. READ returns backend-owned accepted targets, not measured positions.

Missing calibration rejects target reads and writes. Fault rejects writes but permits reads when calibration is valid. Inactive writes prepare values; active writes submit a complete candidate batch. Validation and backend errors preserve accepted targets. Technical errors latch Fault; invalid angles do not. A successful ACK confirms acceptance, not physical motion or completed activation of a pending signal plan.

RegisterService decodes the whole batch before one backend call and stages complete READ data before modifying the caller buffer. The shared FakeOutput uses synthetic calibrations for host tests; production ServoBank uses its configured Calibration values. Journal-backed initialization is not part of this service yet.

## Direct ServoBank backend

[servo_bank_backend.rs](src/servo_bank_backend.rs) implements OutputBackend directly for ServoBank<O, N> when O implements PulseBankLifecycle, without another target store or cached output state. Start delegates to start_prepared; missing/invalid targets become InvalidTargets. Lifecycle and submission errors retain their types in ServoBankBackendError. Stop retains accepted targets; state uses the backend's confirmed logical state.

Unknown and uninitialized targets remain distinct errors. Uninitialized access maps to InvalidState without latching Fault. Inactive writes call prepare_targets; active writes call set_targets. service forwards through ControlRegister and OutputBackend to ServoBank::service and must remain reachable independently of incoming requests. Automatic zero-degree initialization and asynchronous fault recovery are not added.

## Frame processing over a byte transport

[FrameProcessor](src/frame_processor.rs) owns RegisterService, a request StreamParser, one ResponseBuffer<MAX_FRAME_SIZE> and a pending receive-timeout flag. Every poll first services output, then transmits an existing response within its byte budget. Pending response bytes block reception. Newly produced ACK/NACK frames are copied into the response buffer and transmitted on a later poll. The master waits for each response before sending its next request.

Complete buffered requests are drained before an incomplete remainder is discarded on a deferred timeout. The caller detects timeouts and accounts for unread transport data. Receive errors preserve parser contents; transmit errors retain the unsent suffix. FrameProgress reports receive/transmit counts, processed requests and receive_empty; false receive_empty can mean no read was attempted. Byte budgets bound transport calls, not elapsed processing time. Transport acceptance does not mean physical UART completion.

## Modules and tests

| File | Responsibility |
| --- | --- |
| [service.rs](src/service.rs) | Dispatch, validation and ACK/NACK payloads |
| [control.rs](src/control.rs) | Enable transitions, state guards and Fault |
| [output.rs](src/output.rs) | OutputBackend and typed failures |
| [servo_bank_backend.rs](src/servo_bank_backend.rs) | Direct ServoBank adapter |
| [frame_processor.rs](src/frame_processor.rs) | Bounded protocol processing |
| [test_support.rs](src/test_support.rs) | Shared host-test backend |

The current 75 tests cover 16 service cases, 16 control cases, 8 target cases, 12 ServoBank cases, 11 wire-pipeline cases and 12 FrameProcessor cases. Pipeline tests check CRC, discarded headers, NACK errors, order, atomic target rejection and timeout recovery. Processor tests preserve budget, backpressure, retry and deferred-timeout coverage, including separate WRITE ACK and full-target READ responses.

From the repository root:

```bash
cargo test --manifest-path firmware/servo-registers/Cargo.toml --lib --locked
python tools/dev.py fmt
python tools/dev.py verify
python tools/dev.py doc
```

## Physical UART verification

The [uart_registers example](../pico/hardware_tests/README.md#uart-register-requestresponse-test) connects UartTransport and FrameProcessor to NoServoOutput. It reports version 2, Status=0 and ServoCount=0; there is no physical servo backend or journal-loaded calibration. The example applies a provisional 100-ms receive-idle timeout. On 2026-11-03 the owner reported all 21 scenarios passing on real hardware; see [wire vectors](../../docs/uart-register-test-vectors.md).

The owner also reported all 75 package tests, 82 protocol-package tests, and full local verification with 462 tests and 17 checks passing. CI for this revision has not yet been reported. These results do not establish protocol-to-real-PIO integration, flash-operation timing, UART hardware-error injection or normal-application behavior.
