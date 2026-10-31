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

The map is exported as REGISTER_REGIONS for parser configuration. Index/access/length checks are repeated at the service boundary; payload length is checked before indexed access or side effects. READ failures return the normal LastError register. WRITE/SET/CLR produce no response. Successful operations do not clear a previous LastError. A too-small local response buffer leaves registers and buffer unchanged; ServiceError represents that caller-side failure rather than a protocol error.

Error codes currently are None=0, UnknownIndex=1, UnsupportedAccess=2, InvalidLength=3, OutOfBounds=4, InvalidValue=5, NotConfigured=6, InvalidState=7, OutputError=8. They describe this initial implementation, not a separately frozen production protocol.

## Control and output contract

ControlRegister owns an OutputBackend. Bit 0 of Control is OUTPUT_ENABLE; other bits are reserved and rejected even for CLR. Status currently reports CALIBRATION_VALID at bit 0, OUTPUT_ACTIVE at bit 1 and a latched FAULT at bit 3. BUSY is not implemented. Calibration validity is supplied at construction; this does not load or independently validate actual calibration data.

WRITE replaces Control, SET applies OR and CLR clears selected bits. The complete request is checked before a backend call. Enable requires calibration and no fault. Backend start validates the prepared initial targets; InvalidTargets rejects the request without a technical fault. Technical start/stop errors latch Fault and preserve the previously accepted Control value. Status reads OutputBackend::state, including after an error, so it may differ from the requested enable state. Repeated enable/disable avoids redundant backend start/stop calls. Disabling remains possible in Fault, but does not clear Fault. Clearing LastError also does not clear Fault.

OutputBackend provides start, stop and a reliable logical Active/Inactive state. It does not report mechanical position or servo power. The concrete future adapter must uphold the post-error state contract. The package currently has only a shared test backend, which records calls and simulates failures leaving output either active or inactive. No real output adapter is implemented or exercised here.

## Modules and tests

- [service.rs](src/service.rs): fixed register dispatch, responses and LastError.
- [control.rs](src/control.rs): enable transitions and control-related status.
- [output.rs](src/output.rs): output backend contract and typed start failures.
- [test_support.rs](src/test_support.rs): shared backend for host tests only.

From the repository root:

```bash
cargo test --manifest-path firmware/servo-registers/Cargo.toml --lib --locked
python tools/dev.py fmt
python tools/dev.py verify
python tools/dev.py doc
```

On 2026-10-31 the owner reported 41 passing host tests after the intended failing phases: 14 original service tests, 16 control tests and 11 service/control integration tests. Coverage includes canonical error responses, no reply/output-buffer writes for modifying requests, error retention/explicit clearing, reserved bits, missing calibration, invalid initial targets, actual output state after failures and fault persistence. The owner subsequently confirmed all 17 full verification checks and all 9 shared documentation checks passing. CI for this register-service revision is pending. No tests were executed by the assistant.

Target-angle and calibration regions, invalidation, Busy/asynchronous status events, real output/flash backends, UART delivery and the firmware loop remain future work. This package is not included by the normal Pico firmware yet.
