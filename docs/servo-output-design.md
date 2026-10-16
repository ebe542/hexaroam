# Multi-channel servo output design

Status: working design, 2026-10-16. Partially implemented and host-tested; the new output path is not hardware-verified. The committed Pico firmware still uses one state machine per output. This document proposes its replacement; it does not describe current firmware behavior.

## Scope and command contract

Use one RP2040 PIO block and one state machine to generate signals for N configured servo GPIOs. DMA feeds a repeating signal plan through the TX FIFO. Channel count is bounded by available pins and statically allocated storage, not by the number of state machines. Start with one CPU core.

The controller knows channel IDs, calibration, angle limits, and physical output assignments. Geometry, gait generation, kinematics, navigation, and behavior remain outside this output layer. Protocol and local applications should use the same command interface.

Accept one or several channel/angle pairs as a single request. Validate the complete request before submitting output changes. Unknown or duplicate IDs and invalid angles reject the request without output changes. Unspecified channels retain their latest accepted targets. Distinguish accepted targets, targets active in the output, and measured positions. No measured position is available with the current hardware.

Initially all servo outputs are Low. Activation requires a valid initial target for every configured channel. A zero-degree command is a calibrated pulse, not an inactive output. Configuration changes while active are outside the initial implementation.

## Signal plan

All active channels start their pulse together. Sort pulse end times and combine equal times into one GPIO state change. Each logical entry contains a complete GPIO state mask and a positive duration in whole microseconds. Durations sum to 20,000 us. A plan has at most N+1 entries: one initial state and up to N distinct falling-edge states, including the final all-Low interval.

Example for GP0, GP1, and GP2:

| Mask GP2..GP0 | Duration |
| --- | --- |
| 111 | 1000 us |
| 110 | 500 us |
| 100 | 500 us |
| 000 | 18000 us |

Rebuild a plan only when accepted targets change. Repeat the existing plan otherwise. The CPU computes the plan; PIO does not compare servo pulse widths.

## Proposed transfer encoding and PIO program

Each entry uses two u32 words: GPIO state mask with bit 31 set, followed by a positive delay counter. Bit 31 distinguishes an all-Low mask from an empty-FIFO fallback of zero. This encoding targets RP2040, with output base 0 and output count 32; it is not a portable GPIO mapping scheme. Only configured servo pins may be physically connected to this PIO output. Other state machines must not interfere with these outputs.

Disable autopull. Both nonblocking pulls use X=0 as their empty-FIFO fallback. Check both words before changing pins. The checks detect absent/zero words, not arbitrary corruption or a malformed nonzero mask.

```asm
.wrap_target
    set x, 0
    pull noblock
    mov y, osr
    jmp !y fault
    mov isr, osr
    pull noblock
    mov y, osr
    jmp !y fault
    mov osr, isr
    out pins, 32
delay:
    jmp y-- delay
.wrap

fault:
    mov osr, null
    out pins, 32
    irq 0
halt:
    jmp halt
```

The proposed program occupies 15 instruction slots. Between successive OUT PINS instructions, the countdown executes counter+1 instructions and the next entry preparation executes 10 instructions. Thus the interval is counter+11 PIO cycles, provided data is available.

At a proposed 25 MHz state-machine clock:

```text
counter = duration_us * 25 - 11
```

A 1-us interval uses counter 14: 15 countdown cycles plus 10 preparation cycles equals 25 cycles. The 25 cycles/us come from the chosen 25 MHz PIO clock, not from the program length. The eleven fixed cycles are ten next-entry instructions plus the final countdown check. See the [instruction-by-instruction timing derivation](../firmware/servo-pio/README.md#timing-derivation) for the counting boundaries and clock-divider example. Require an exactly achievable clock divider and checked encoding arithmetic. The PIO assembly has been accepted by a Pico build. Physical timing and DMA throughput still require verification. Adding instructions changes the formula.

For 18 channels, 19 entries require 152 bytes of transfer data per plan; two such payload buffers require 304 bytes, excluding metadata and any additional boundary markers.

## DMA and buffer ownership

Use two stable, statically allocated payload buffers. A buffer is Free, Writing, Ready, or DmaSource. Never mutate Ready or DmaSource data. Publish a completed plan and change ownership with appropriate interrupt-safe synchronization.

A DMA transfer sends exactly one complete plan, paced by the PIO TX request. On completion, select a Ready plan for the next transfer or repeat the previous plan. No partial plan is submitted. FIFO ordering preserves complete plan order even when the next plan is prefetched before the current period has finished.

A previous buffer becomes writable only after its final DMA read and after ensuring it will not be selected for repetition. DMA completion is not GPIO activation: words may remain in the FIFO or PIO registers. Maintain distinct plan identity for transfer selection and actual output activation. Return Busy when there is no writable buffer; do not queue historical commands.

The final all-Low interval provides time to rearm DMA. With a maximum pulse of 2000 us, the nominal interval is 18000 us. The supported pulse range must retain a quantified rearm margin. Do not reuse the old driver's near-20-ms technical limit. Analyze worst-case FIFO progress, interrupt latency, and DMA service time before accepting this scheme.

## Underflow and restart

An empty FIFO while PIO is counting is not itself a fault. A missing word at a required pull is a fault. The program forces outputs Low, sets PIO IRQ flag 0, and loops permanently. CPU interrupt routing is configured separately. Clearing the flag does not resume output.

Low is a defined electrical failure state, not proof of a mechanically safe posture or a particular servo torque response.

Explicit reactivation disables the state machine, stops DMA and waits for completion/abort, clears FIFO and stale interrupt flags, resets execution state and program counter, selects a complete valid plan, starts DMA and prefills the FIFO, resets the clock divider, and enables PIO. Use the same controlled startup path for initial activation and recovery. Do not resume in the middle of a previous stream.

## Verification and remaining decisions

Before replacing the current driver:

- Verify the PIO assembly, wrap locations, shift configuration, GPIO ownership, and cycle calculation.
- Test plan generation for N channels, duplicate times, arbitrary valid pin assignments, exact period sum, and invalid inputs.
- Specify the supported pulse range and calculate the DMA rearm deadline.
- Define interrupt-safe buffer transitions and exact acceptance semantics.
- Choose how to identify actual period/plan activation; any added PIO instructions require a revised timing calculation.
- Verify missing first and second words force Low and latch a fault without CPU assistance.
- Verify startup, unchanged-plan repetition, complete-plan replacement, and explicit recovery on the oscilloscope.

The existing Angle and Calibration remain useful. ServoBank's per-channel sequential output backend must be revised for whole-plan submission. The temporary start_pair approach has been discarded; multi-state-machine synchronization is not required for this design.

## References

- [RP2040 datasheet, PIO chapter](https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf): instruction behavior, FIFO semantics, and GPIO mapping.
- [Pimoroni PWM cluster PIO](https://github.com/pimoroni/pimoroni-pico/blob/main/drivers/pwm/pwm_cluster.pio): reference for GPIO-mask/delay streaming.
- [Pimoroni PWM cluster implementation](https://github.com/pimoroni/pimoroni-pico/blob/main/drivers/pwm/pwm_cluster.cpp): reference for transition generation and DMA feeding. The proposed checked PIO program above differs from that implementation.

## Implementation progress

SignalPlan generation in servo-signal passed 13 new host tests, with 93 tests in the full local check; the owner subsequently reported green CI for that revision. The checked PIO program is compiled but not installed or executed.

The servo-pio library now encodes individual plan entries, checks configured RP2040 GPIO masks, and rejects zero or unrepresentable durations. The encoder and its error type are internal implementation details, not application-facing APIs. The future output driver will accept complete SignalPlan values and own the GPIO/clock configuration; callers do not perform ASM-specific encoding. The owner reported nine encoder tests passing and a full local check with 102 tests. The owner subsequently reported green CI for the entry encoder revision. Complete-plan encoding is now implemented internally through EncodedSignalPlan: it preserves word order, checks capacity, and returns a payload only after all entries have encoded successfully. Seven additional tests passed, bringing the encoder package to 16 tests and the full local check to 109. The payload still needs stable memory and exclusive ownership for actual DMA reads. DMA integration and physical output verification remain pending; remote CI for complete-plan encoding has not yet been reported. See the [encoder guide](../firmware/servo-pio/README.md).
