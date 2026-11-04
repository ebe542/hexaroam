# Servo register service

Current development status (2026-11-04): the hardware-independent persist_calibration() function now connects FrameProcessor readiness to journal save_if_changed() and completion status. The owner confirmed all eight persistence-integration tests and full local verification with **515 tests and all 17 checks passing**. The register package now contains 118 host tests. Combined physical UART/flash persistence, normal-application integration and CI for this step have not yet been reported.

Hardware-independent no_std register handling above [servo-protocol](../servo-protocol/README.md). RegisterService consumes CRC-checked ParsedFrame values and returns ACK/NACK frames borrowing caller-provided payload storage. The frame codec adds header/CRC bytes; this package does not directly access UART hardware or flash. See the [version-2 contract](../../docs/servo-register-protocol.md).

## Implemented register subset

| Index | Register | Bytes | Access |
| --- | --- | ---: | --- |
| 0x00 | Protocol version; UART fixture reports 2 | 2 | READ |
| 0x01 | Control/backend status | 4 | READ |
| 0x02 | Unassigned; reserved for future restart reason | — | None |
| 0x03 | Control | 1 | READ, WRITE, SET, CLR |
| 0x04 | Servo count | 1 | READ |
| 0x10 | Complete calibration request | 368 | WRITE; READ/SET/CLR rejected |
| 0x20..0x31 | Target angles for servo IDs 0..17 | 4 per servo | READ, WRITE |

REGISTER_REGIONS supplies parser configuration. The service independently revalidates access and payload size before indexed reads or side effects. Successful READ returns ACK with data; successful WRITE/SET/CLR returns an empty ACK. Errors return NACK with the original index and one error byte. LastError was removed. The current handle signature retains Option<Frame>, but every handled request produces a response unless a local ServiceError occurs.

The stream parser discards unknown indices and implausible lengths before dispatch. Direct service callers can still receive UnknownIndex/InvalidLength NACKs. ErrorCode is repr(u8): None=0, UnknownIndex=1, UnsupportedAccess=2, InvalidLength=3, OutOfBounds=4, InvalidValue=5, NotConfigured=6, InvalidState=7, OutputError=8. Local response-capacity errors leave state and caller storage unchanged; modifying operations reserve capacity for a possible one-byte NACK before executing.

## Control and output contract

ControlRegister owns an OutputBackend. Control bit 0 is OUTPUT_ENABLE; reserved bits are rejected even for CLR. WRITE replaces Control, SET applies OR and CLR clears selected bits. Enable requires calibration and no Fault. Backend InvalidTargets rejects startup without a technical fault; technical start/stop errors latch Fault and preserve accepted Control values. Repeated enable/disable avoids redundant calls. Disable remains possible in Fault but does not clear it.

Status reports CALIBRATION_VALID at bit 0, actual OUTPUT_ACTIVE at bit 1 and latched FAULT at bit 3. It reports logical backend state, not measured position or servo power. Calibration validity is supplied at construction. Busy is bit 2; bits 8..15 report the calibration-save result. Restart reason and restart remain unimplemented; calibration transfer is accepted into RAM but does not yet write flash.

## Target-angle registers

Each target occupies one logical index and carries signed i32 little-endian centidegrees. Requests may span consecutive targets within the static region and configured servo count. SET/CLR are prohibited. READ returns backend-owned accepted targets, not measured positions.

Missing calibration rejects target reads and writes. Fault rejects writes but permits reads when calibration is valid. Inactive writes prepare values; active writes submit a complete candidate batch. Validation and backend errors preserve accepted targets. Technical errors latch Fault; invalid angles do not. A successful ACK confirms acceptance, not physical motion or completed activation of a pending signal plan.

RegisterService decodes the whole batch before one backend call and stages complete READ data before modifying the caller buffer. The shared FakeOutput uses synthetic calibrations for host tests; production ServoBank uses its configured Calibration values. Journal-backed initialization is not part of this service yet.

## Direct ServoBank backend

[servo_bank_backend.rs](src/servo_bank_backend.rs) implements OutputBackend directly for ServoBank<O, N> when O implements PulseBankLifecycle, without another target store or cached output state. Start delegates to start_prepared; missing/invalid targets become InvalidTargets. Lifecycle and submission errors retain their types in ServoBankBackendError. Stop retains accepted targets; state uses the backend's confirmed logical state.

Unknown and uninitialized targets remain distinct errors. Uninitialized access maps to InvalidState without latching Fault. Inactive writes call prepare_targets; active writes call set_targets. service forwards through ControlRegister and OutputBackend to ServoBank::service and must remain reachable independently of incoming requests. Automatic zero-degree initialization and asynchronous fault recovery are not added.

## Pending calibration data

[PendingCalibration<N>](src/pending_calibration.rs) calls servo-storage::decode with the caller's Checksum implementation, preserving FormatError details. It then rejects the first missing ServoId in ascending order. Success owns the decoded array in RAM, independently of the receive buffer, and exposes only an immutable reference matching journal append's existing calibration argument. No flash access, journal slot/sequence selection, output changes or Busy transitions occur here.

The twelve tests in [test_pending_calibration.rs](src/test_pending_calibration.rs) cover distinct calibrations including reversed pulse direction, arbitrary input order, ownership after input overwrite/drop, every missing servo, empty data, duplicate/unknown IDs, magic, lengths, CRC, invalid calibration values and a complete eighteen-servo set. The owner observed 12 failing tests before implementation, followed by all 12 passing. Completeness is relative to the const-generic N selected by the caller; this type does not choose the configured servo count.

## Calibration request acceptance and completion

Register 0x10 accepts exactly 368 payload bytes: the existing four-byte format header, eighteen 20-byte entries and inner CRC32. The outer protocol CRC remains separate. Unknown indices and implausible lengths are still discarded by the stream parser; direct service calls can return InvalidLength NACK. A shorter partial calibration does not reach the wire service for an InvalidValue reply. The register map permits READ for structural plausibility, but the service rejects calibration READ, SET and CLR with UnsupportedAccess.

Acceptance requires inactive actual output and no pending request. Busy or active output returns InvalidState; invalid format, dataset CRC, duplicate/missing IDs or invalid calibration values return InvalidValue when the frame length is plausible. The service reserves NACK response capacity before accepting anything. It decodes with SoftwareCrc32 through PendingCalibration's Checksum interface; the RAM type itself remains checksum-adapter independent.

Success installs the owned PendingCalibration<18>, resets the prior completion result and returns an empty ACK. Busy is derived from the presence of that request rather than a second mutable flag. Completeness currently uses the fixed eighteen-servo firmware capacity, not the active ServoCount, which may be zero before loading calibration. It does not update active calibration or ServoCount.

While Busy, otherwise valid modifying requests are rejected before target-count or backend processing. Ordinary READ operations remain available. Invalid requests do not replace the pending data or prior operation result. pending_calibration() returns an immutable reference without changing Busy. complete_calibration(result) records the result before removing the request; a call without a request returns NoPendingCalibration without changing state. After either success or failure the request is released; retry requires submitting a new complete dataset.

Status bits 8..15 use 0=None, 1=Success, 2=StorageError and 3=VerificationFailed. None is reported initially and after accepting a new request. The result persists after completion; rejected requests do not overwrite it. Fault remains an independent output-status bit.

This step does not write flash. The program loop must later wait until the complete ACK has been accepted by the transport and the software response buffer is empty before invoking the journal writer. A separate UART-idle interface is not required. FrameProcessor now forwards completion and exposes calibration_ready() only when its software response buffer is empty. The caller must perform persistence before reporting completion; the API does not independently verify that a journal write took place. An available pending request alone is not a write authorization.

## Frame processing over a byte transport

[FrameProcessor](src/frame_processor.rs) owns RegisterService, a request StreamParser, one ResponseBuffer<MAX_FRAME_SIZE> and a pending receive-timeout flag. Every poll first services output, then transmits an existing response within its byte budget. Pending response bytes block reception. Newly produced ACK/NACK frames are copied into the response buffer and transmitted on a later poll. The master waits for each response before sending its next request.

Complete buffered requests are drained before an incomplete remainder is discarded on a deferred timeout. The caller detects timeouts and accounts for unread transport data. Receive errors preserve parser contents; transmit errors retain the unsent suffix. FrameProgress reports receive/transmit counts, processed requests and receive_empty; false receive_empty can mean no read was attempted. Byte budgets bound transport calls, not elapsed processing time. Transport acceptance does not mean physical UART completion.

## Calibration dispatch after response acceptance

calibration_ready() returns an immutable pending request only when no encoded response bytes remain in the software buffer. Partial transmission, backpressure, transmit errors and a newly queued status response delay readiness. Repeated readiness checks do not consume the request or clear Busy. There is no separate UART-idle query; software-buffer completion does not prove physical transmission or receipt by the master.

complete_calibration() forwards Success, StorageError or VerificationFailed to RegisterService, where the result is retained and the request released. The caller must save the request before invoking completion. Eight host tests in [test_calibration_dispatch.rs](src/test_calibration_dispatch.rs) cover initial/queued states, partial ACKs, backpressure, retry after transmission errors, status-response gating and completion, using the parent FrameProcessor test module's simulated transport. No journal operation occurs in those tests.

## Calibration persistence integration

[persist_calibration()](src/calibration_persistence.rs) borrows the FrameProcessor, a JournalStorage backend and a Checksum adapter. It returns Ok(false) without any storage access when no request is ready, including while ACK bytes remain queued. For a released request it calls save_if_changed(); identical newest data succeeds without another program/erase or sequence increment. Changed data uses the existing append and verification logic.

Successful persistence reports CalibrationSaveResult::Success. JournalError::VerificationFailed maps to the distinct verification result; other journal errors map to StorageError. The original JournalError remains available to the caller through CalibrationPersistenceError::Journal. Completion releases the request and clears Busy on either success or failure, so retries require a new request and do not happen implicitly. Unexpected completion errors are returned separately. Ok(true) means the request completed successfully; it does not necessarily mean flash was written.

The eight tests in [test_calibration_persistence.rs](src/test_calibration_persistence.rs) combine the real request parser, FrameProcessor, register service and journal logic with simulated UART/storage. They verify idle and queued/partial ACK guards, successful saving, identical retry, changed data, read/write errors, failed verification and absence of automatic retry. No GPIO or real flash operation is exercised. The future program loop must call this function after poll; physical UART-idle waiting is not required by the chosen design.

## Modules and tests

| File | Responsibility |
| --- | --- |
| [service.rs](src/service.rs) | Dispatch, validation and ACK/NACK payloads |
| [control.rs](src/control.rs) | Enable transitions, state guards and Fault |
| [output.rs](src/output.rs) | OutputBackend and typed failures |
| [servo_bank_backend.rs](src/servo_bank_backend.rs) | Direct ServoBank adapter |
| [frame_processor.rs](src/frame_processor.rs) | Bounded protocol processing |
| [test_support.rs](src/test_support.rs) | Shared host-test backend |

The current 118 register-package tests comprise 16 control, 19 service, 8 target, 12 ServoBank, 11 wire-pipeline, 28 FrameProcessor integration cases (including eight dispatch and eight persistence cases), 12 pending-calibration and 12 calibration-request cases. New request tests exercise state guards, immutable RAM ownership, read availability during Busy, atomic rejection, response capacity, completion and result retention. Physical UART timing and flash execution are separate checks.

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

## Retry-safe journal saving (2026-11-04)

The storage layer now provides `save_if_changed()`. The host-tested persist_calibration() function uses it to acknowledge already persisted identical data without another flash write or sequence increment. Busy rejects concurrent modifications; a retry after completion can be accepted and compared against the newest valid record. ACK delivery is not guaranteed by draining the transmit buffer, so equality-based saving is required independently of UART timing. Ten new host tests and full local verification (499 tests, all 17 checks) passed according to the owner. The persistence coordinator now uses this function with FrameProcessor readiness; the UART hardware example does not yet invoke it.
