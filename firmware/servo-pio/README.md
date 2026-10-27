# Servo PIO encoding

`hexaroam-servo-pio` is a `no_std` library for the RP2040-specific representation of servo signal plans. It implements internal encoding of individual entries and complete SignalPlan values. The encoding module is private; encode_entry and EncodingError are crate-visible and are not exported to applications. The hardware-only public interfaces support both repeated fixed plans and replacement of complete SignalPlan values using two buffers. It depends on servo-signal and assembles its PIO program on the host. RP2040 HAL code is enabled only for ARM bare-metal builds, so pure encoding and buffer tests still run on the host.

## Internal entry encoding

`encode_entry(entry, allowed_gpio_mask)` returns two `u32` words: the GPIO state mask with marker bit 31 set, followed by the delay counter. The marker keeps an all-Low state distinct from the PIO program's zero fallback for a missing FIFO word.

The allowed mask must be nonzero and contain only RP2040 GPIO0 through GPIO29. An entry may contain only configured bits; a zero entry mask is valid. The board configuration must additionally exclude unavailable or reserved GPIOs. The generic SignalPlan supports 32 mask bits, but this encoder deliberately restricts physical outputs to the RP2040 GPIO range.

For the proposed checked PIO program at 25 MHz:

```text
counter = duration_us * 25 - 11
```

The eleven cycles include preparation of the next entry and the final countdown instruction. Any change to the program or clock requires reviewing this encoding. A one-microsecond duration yields counter 14. Zero durations are rejected. Arithmetic uses u64 intermediates and checked conversion to u32. The largest representable whole-microsecond duration is 171,798,692 us; this is an encoding limit, not an allowed servo pulse width.

Errors are `InvalidGpioMask`, `UnconfiguredGpio`, `ZeroDuration`, and `DurationOverflow`. No hardware is accessed and no FIFO writes occur.

## Complete-plan encoding

The internal `EncodedSignalPlan<WORD_CAPACITY>` owns a fixed-size array of contiguous u32 words and the number of valid words. Its constructor accepts a complete SignalPlan and the configured GPIO mask. Each entry is encoded in order as mask then counter using the existing entry encoder. `words()` exposes only the initialized payload, excluding spare capacity.

Capacity counts words, not channels or logical entries. Each logical entry requires two words; a worst-case 18-channel plan therefore requires 38 words. Equal pulse end times reduce the actual requirement. The capacity check uses `entries.len() > WORD_CAPACITY / 2` to avoid multiplication overflow. Insufficient space returns `InsufficientCapacity`.

Construction happens in a local buffer. Any entry error rejects the whole result, including an overflow in the final all-Low interval. No partially encoded plan is returned and no hardware is accessed.

This encoded value alone is not a DMA ownership mechanism. OutputBuffer stores it; the fixed-output driver receives static storage and freezes its payload as a static read-only slice before DMA starts. Encoding alone does not start output or establish frame-boundary activation.

## Timing derivation

The two constants have different origins: 25 is the selected number of PIO clock cycles per microsecond; 11 comes from the instruction sequence in [program.rs](src/program.rs).

### Why 25 cycles per microsecond?

A proposed state-machine clock of 25 MHz means:

```text
25,000,000 cycles / second = 25 cycles / microsecond
one PIO cycle = 40 ns
```

This is a design choice, not an instruction count. It permits whole-microsecond durations while leaving enough cycles for both data checks and GPIO output, even for adjacent events only 1 us apart. With a 125 MHz system clock, for example, an integer divider of 5 produces 25 MHz. PreparedOutput checks that the system clock supports an integer divider and configures it; the current demo installs the program at this frequency. The encoder assumes exactly 25 MHz and does not configure it.

### Why eleven overhead cycles?

Measure from one successful `out pins, 32` output update to the next. Count the delay loop after the current update and all ten instructions through the next output update. The current output instruction is the starting boundary and is not counted again.

| Next-entry instruction | Cycles | Purpose |
| --- | --- | --- |
| `set x, 0` | 1 | Establish the empty-FIFO fallback |
| `pull noblock` | 1 | Fetch the mask word |
| `mov y, osr` | 1 | Make the word available to the zero check |
| `jmp !y fault` | 1 | Check for a missing mask; normal path does not branch |
| `mov isr, osr` | 1 | Preserve the mask while fetching the counter |
| `pull noblock` | 1 | Fetch the delay counter |
| `mov y, osr` | 1 | Load the countdown register |
| `jmp !y fault` | 1 | Check for a missing counter; normal path does not branch |
| `mov osr, isr` | 1 | Restore the mask |
| `out pins, 32` | 1 | Apply the next GPIO state |
| **Preparation through next output** | **10** | |

With Y initially equal to counter C, `jmp y-- delay` executes C times with a taken branch, followed by one final execution when Y is zero and the branch is not taken. Therefore the delay loop takes C+1 cycles. Hardware wrap adds no instruction or extra cycle.

```text
interval_cycles = (C + 1) + 10
                = C + 11

interval_cycles = duration_us * 25

C = duration_us * 25 - 11
```

Thus eleven is ten next-entry instructions plus the final countdown check. It is not eleven additional instructions outside the loop.

For 1 us, C=14: the loop takes 15 cycles and next-entry preparation takes 10, totaling 25 cycles. For 1000 us, C=24,989: 24,990 loop cycles plus 10 preparation cycles equals 25,000 cycles, or 1000 us.

This calculation covers the normal path with valid words, no instruction delay fields, and autopull disabled. Underflow branches to the separate Low/fault path; it does not preserve the normal interval. The first output after startup also has a preparation latency and is not itself a complete inter-output interval. Initial approximate pulse widths and synchronous rising edges have been observed; precise timing and a new period measurement remain unverified.

## Files and verification

| File | Purpose |
| --- | --- |
| src/lib.rs | no_std declaration and private encoding module |
| src/encoding.rs | Internal entry and complete-plan encoding |
| src/test_encoding.rs | Sixteen host encoding tests |
| src/program.rs | Multi-channel PIO assembly |
| src/output.rs | Hardware-only prepared output and fixed DMA servicing |
| src/buffer.rs | Public opaque storage with internal loading |
| src/test_buffer.rs | Five host buffer tests |
| src/pending_signal_plan.rs | Validation and preparation of a replacement plan |
| src/test_pending_signal_plan.rs | Eleven host submission and restart-preparation tests |
| src/signal_plan_dma_source.rs | Exclusive static buffer ownership for DMA reads |
| src/switching_output.rs | Two-buffer plan submission, DMA handover, and explicit restart |
| src/dma_abort.rs | Bounded, channel-specific DMA abort with RP2040-E13 handling |
| Cargo.lock | Locked dependency resolution; commit this file |

From the repository root:

```bash
python tools/dev.py verify
```

For this package only:

```bash
cd firmware/servo-pio
cargo test --lib --locked
```

On Ubuntu use `python3` if `python` is unavailable. Run Cargo from this package's directory to avoid the Pico firmware's embedded target configuration.

The nine tests cover marked masks, all-Low encoding, the minimum duration, GPIO29, invalid configuration masks, unconfigured bits, zero duration, the maximum encodable duration, and overflow. The owner reported a successful full local check with 102 tests, Rust formatting checks for four packages, and the existing Pico build. The owner subsequently reported green CI for the entry encoder revision.

Seven additional tests cover complete word ordering, hidden spare capacity, insufficient capacity, invalid configuration masks, unconfigured outputs, final-entry overflow, and capacity based on actual entries rather than channel count. The owner reported 16 encoder tests passing and a successful full local check with 109 tests. The owner subsequently reported green CI for complete-plan encoding.

## Fixed-plan hardware integration

The Pico demo now depends on this library. It allocates OutputBuffer<6> once using cortex_m::singleton!, constructs a three-entry SignalPlan, and prepares PIO0 SM0 with GP0 and GP1. One DMA channel supplies both outputs. No per-servo state-machine synchronization is needed.

Public hardware types are PreparedOutput, FixedOutput, OutputInitError, and OutputStartError. PreparedOutput::new owns the configured pins and a stopped state machine. start_fixed validates/encodes the plan in static buffer storage, prefills one complete FIFO entry, starts DMA for the remaining words, and starts PIO. Encoding details remain private. Start failures are reported as InvalidPlan or InsufficientCapacity without exposing ASM-specific errors.

FixedOutput owns the running state machine, pins, current DMA transfer, and a static read-only payload reference. service polls transfer completion and rearms the same channel with the full plan. The application must call it regularly. The demo uses timestamp-based LED updates rather than blocking delays. DMA completion is not an indication of GPIO activation; FIFO data may still be pending.

Five buffer tests cover empty storage, a complete payload, shorter replacements hiding old words, and preservation of previous data on encoding or capacity errors. The owner reported 114 tests passing in the full local check. On 2026-10-17, the owner observed approximately 1 ms High on GP0 and 2 ms High on GP1 with synchronous rising edges. A new measured period, exact timing, deliberate underflow behavior, recovery, and 18-channel operation remain unverified. Remote CI for this DMA integration has not yet been reported.

The PIO underflow branch sets outputs Low, raises flag 0, and loops. Fault reporting and controlled recovery are not implemented in the current demo. The program must not be resumed merely by clearing the flag. Low does not establish a safe mechanical posture.

The fixed-output path permanently freezes its payload. A separate SwitchingOutput now provides dynamic submission and two-buffer ownership as described below. Explicit activation confirmation and a general DMA rearm latency bound remain pending. The fixed experiment permanently freezes one payload; it does not implement the planned writable/ready buffer cycle. Host tests do not establish physical signal timing or electrical servo compatibility.

See the [multi-channel output working design](../../docs/servo-output-design.md) for the PIO program, timing derivation, buffer ownership, and fault/restart requirements.

## Reproducible underflow tests

The [Pico hardware-test programs](../pico/hardware_tests/README.md) now separately exercise a missing next entry and a missing final counter. The owner reported the missing-entry test behaving as described and the missing-counter test entering fault mode immediately without visible startup edges. These are manual hardware observations; exact fault latency was not measured.

The hardware-tests feature gates start_with_missing_final_counter and its initial-transfer truncation mode. Both normal and test starts share the same initialization path. Without the feature, the truncation mode and test start method are not compiled. The ordinary fixed-plan demo remains unchanged.

The full local check now includes 121 host unit tests; hardware-test builds and UF2 generation were separately reported successful. Fault recovery is still pending. Dynamic plan changes are covered by the separate switching-output path below. Remote CI for this revision has not yet been reported.

## Two-buffer signal-plan replacement

SwitchingOutput::start takes a PreparedOutput, one DMA channel, two distinct static OutputBuffer values, and a complete initial SignalPlan. The initial plan must use every configured output. Its period and GPIO mask become the contract for later submissions. FixedOutput and its existing hardware tests remain available unchanged.

submit_plan accepts a complete replacement plan. PendingSignalPlan first rejects an occupied ready slot with Busy, then checks period and output-mask equality, and finally encodes the plan into the spare buffer. Only successful encoding marks it ready. Failures leave the previous buffer contents and readiness unchanged. The seven host tests cover initial state, successful preparation, Busy preservation, period mismatch, output mismatch, capacity failure, and complete multi-output preparation.

SignalPlanDmaSource holds an exclusive static reference and a validated nonempty word range. Its hardware-only unsafe ReadTarget implementation exposes the stable payload address and length to the HAL. It provides no mutation while owned by a DMA transfer. The initial range skips the prefetched entry; repetitions and replacements transfer the complete plan.

service polls completion and calls transfer.wait only after is_done. The returned source releases exclusive access to the completed RAM buffer. If a replacement is ready, the spare becomes the next DMA source and the completed buffer becomes writable. Otherwise the completed buffer is retransmitted. FIFO words are already copied and do not depend on subsequent changes to their old RAM buffer. Entire plans are submitted in order; the PIO program and its cycle formula are unchanged.

Success from submit_plan means accepted for a future output period, not active at the GPIOs. Busy clears when the prepared plan is selected for DMA, not upon physical activation. No active-plan acknowledgement is available. Submission and service currently run in one execution context through exclusive mutable access; interrupt-based concurrent submission has not been implemented.

The owner reported a successful full local verification with 128 tests and successful hardware-test builds. On 2026-10-17, switching_signal_plans ran as expected: synchronous rising edges and alternating High durations on GP0 and GP1. Its immediate second submission asserts Busy; the reported continuing execution is consistent with that assertion passing. Exact period measurements, brief update-glitch exclusion, worst-case latency, 18-channel output, and controlled recovery remain open. The owner subsequently reported green CI for this switching revision. Submission rejection after underflow is covered by the later fault-state step below.

The normal application still uses the fixed-plan demo. Dynamic operation is currently exercised by the separate hardware-test program, not by ServoBank or an external protocol.

## Persistent PIO fault state

SwitchingOutput now reads reserved PIO IRQ flag 0 internally. is_faulted latches an observed flag in the driver; clearing the hardware flag later does not clear this state. The read-only PAC register access selects the PIO block by P::id and does not acknowledge the flag. Flag 0 must be exclusively reserved for this output within the block.

start rejects an already-set flag with OutputStartError::Faulted before encoding or starting transfers. submit_plan checks the latched/hardware fault first and returns OutputSubmissionError::Faulted, ahead of Busy or plan validation, without modifying the pending plan. service checks the same state before taking the DMA transfer or swapping buffers and returns immediately on fault.

A transfer already running at the time of detection is not aborted. Its source remains owned by the transfer in the driver; the fault path does not wait for a transfer that may be stalled behind halted PIO. No further transfer is rearmed after a fault is detected. This is a polling observation, not an atomic guarantee against a hardware fault occurring after a particular check.

On 2026-10-17 the owner reported all requested checks passing and expected execution of faulted_signal_output. That hardware test verifies Faulted taking precedence over an occupied ready slot, persistence after hardware-flag acknowledgement, and continued Low output when servicing resumes. It does not exercise the stale-flag startup rejection separately. The existing unit-test total remains 128; this firmware test is manually evaluated, not counted as a host test. Remote CI for this revision is pending.

Clearing the hardware flag alone is not recovery: PIO remains in its halt loop and the driver remains faulted. Explicit recovery is now available through restart, described below.

## Explicit restart after a fault

`SwitchingOutput::restart(&signal_plan)` is allowed only after a detected or latched fault. Otherwise it returns `OutputRestartError::NotFaulted`. The replacement must have the original period and complete output set. Preparation validates and encodes the whole plan before changing hardware; it may replace an already-ready pending plan. Validation failure preserves the previous payload, readiness, and fault state. Four new host tests cover ready-plan replacement and preservation after period, output, and capacity errors.

After validation, the driver aborts its owned DMA channel while retaining the transfer and source buffer inside the driver. The internal abort helper disables both interrupt routes for that channel using atomic-clear aliases, requests CHAN_ABORT, and polls at most 100,000 times. Due to RP2040-E13, it checks BUSY as well as the channel's abort bit. Once stopped, it clears only that channel's raw completion interrupt. Other DMA channels are not reset. This driver uses polling; its channel interrupt routes remain disabled.

If the polling budget expires, `DmaAbortTimeout` leaves the driver faulted with its DMA resources still bound. The validated restart plan may already occupy the pending slot. No FIFO reset or PIO restart occurs. The budget counts polling attempts, not milliseconds.

After a confirmed abort, HAL wait recovers channel, source, and TX ownership and applies its memory fences. The driver clears old FIFO words, prefills the new plan's first complete entry, starts DMA for the remaining words, and makes the former DMA buffer the new writable slot. It clears reserved PIO flag 0 and uses the HAL restart operation to pause execution, reset internal execution state, jump to the installed program's wrap target, and enable output. The cached fault is then cleared. Normal underflow keeps pins Low during preparation. Success means restart was initiated, not that output activation was measured or acknowledged.

The initial period and GPIO allocation remain fixed. Restart does not resume an old stream or automatically execute the previously pending plan. Validation errors include PeriodMismatch, OutputMismatch, InvalidPlan, and InsufficientCapacity. The normal application still uses FixedOutput; this restart API belongs to SwitchingOutput.

### Manual recovery checks

On 2026-10-17 the owner reported 149 passing unit tests and successful execution of both recovery programs:

- `restart_signal_output`: five seconds of initial pulses, a real underflow, two seconds Low, then the replacement pulse pattern. Assertions reject restart while healthy and reject invalid period/output requests while faulted. LED behavior matched the expected phases.
- `restart_active_dma`: an injected fault disables the state machine and forces Low. The test starts a six-word transfer into the four-word TX FIFO without consuming words. Its feature-gated hook confirms FIFO full and DMA active; assertions confirm DMA remains active through invalid restart attempts and immediately before successful recovery. Signals and LED behavior were reported correct.

`dma_is_active` and `force_fault_with_active_dma` exist only with the `hardware-tests` feature. The hook is deliberate fault injection, not underflow detection. See the [hardware-test guide](../pico/hardware_tests/README.md#controlled-restart-tests).

The forced timeout return path is covered by the later zero-budget test below. Exact restart latency, brief-glitch exclusion, and 18-channel operation remain unverified. Manual programs are excluded from the 149 unit-test count. CI for this recovery revision has not yet been reported.

## SignalPlanOutput integration

SwitchingOutput implements the hardware-independent SignalPlanOutput trait from servo-signal. Trait calls forward explicitly to the existing submit_plan and service methods, preserving complete-plan validation, Busy/Faulted behavior, buffer ownership, and polling-based DMA handover. This adds no new hardware behavior by itself and does not expose restart through the trait.

The ServoSignalOutput adapter can therefore own SwitchingOutput while ServoBank owns the adapter. A main-loop bank.service call reaches this driver's service method without exposing GPIO or PIO types to the bank. On 2026-10-26 the owner reported full verification with 169 passing tests after the trait integration. The owner subsequently confirmed the servo_bank_angles signal check and green CI for the angle-test revision. See the [adapter guide](../servo-control/README.md#signal-plan-output-adapter).

## Exhausted abort-budget verification

The production restart method uses the existing 100,000-attempt polling budget. A hardware-tests-only method, restart_with_zero_abort_budget, delegates to the same restart path with a zero budget. This still validates the replacement and requests a real DMA abort, but deterministically returns DmaAbortTimeout without polling completion. has_dma_transfer lets the manual test check that transfer ownership remains bound after that return.

On timeout, the validated replacement may already occupy the pending slot. The driver does not recover the DMA source, clear the FIFO, or restart PIO. service continues to return while faulted, and ordinary submissions remain Faulted. A later normal restart validates its supplied replacement again and confirms DMA inactivity before recovering resources. The hardware may have completed the earlier abort in the meantime; timeout means completion was not confirmed within the allowed polling attempts, not that DMA is necessarily still active.

On 2026-10-27 the owner reported 169 passing unit tests and successful signal and UART checks for restart_abort_timeout. The program confirms the timeout result, fault persistence, retained transfer, Low interval, and successful normal retry. It does not emulate a physically stuck channel or establish real-time timeout duration. See the [manual procedure](../pico/hardware_tests/README.md#forced-abort-timeout-test). CI for this revision has not yet been reported.
