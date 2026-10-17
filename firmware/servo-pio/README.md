# Servo PIO encoding

`hexaroam-servo-pio` is a `no_std` library for the RP2040-specific representation of servo signal plans. It implements internal encoding of individual entries and complete SignalPlan values. The encoding module is private; encode_entry and EncodingError are crate-visible and are not exported to applications. The hardware-only public interface prepares GPIO outputs and repeatedly emits a fixed SignalPlan through DMA. It depends on servo-signal and assembles its PIO program on the host. RP2040 HAL code is enabled only for ARM bare-metal builds, so pure encoding and buffer tests still run on the host.

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

Dynamic plan submission, two-buffer ownership, explicit activation confirmation, and a general DMA rearm latency bound remain pending. The fixed experiment permanently freezes one payload; it does not implement the planned writable/ready buffer cycle. Host tests do not establish physical signal timing or electrical servo compatibility.

See the [multi-channel output working design](../../docs/servo-output-design.md) for the PIO program, timing derivation, buffer ownership, and fault/restart requirements.

## Reproducible underflow tests

The [Pico hardware-test programs](../pico/hardware_tests/README.md) now separately exercise a missing next entry and a missing final counter. The owner reported the missing-entry test behaving as described and the missing-counter test entering fault mode immediately without visible startup edges. These are manual hardware observations; exact fault latency was not measured.

The hardware-tests feature gates start_with_missing_final_counter and its initial-transfer truncation mode. Both normal and test starts share the same initialization path. Without the feature, the truncation mode and test start method are not compiled. The ordinary fixed-plan demo remains unchanged.

The full local check now includes 121 host unit tests; hardware-test builds and UF2 generation were separately reported successful. Fault recovery and dynamic plan changes are still pending. Remote CI for this revision has not yet been reported.
