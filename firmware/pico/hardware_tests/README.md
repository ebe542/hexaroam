# Pico hardware tests

These are separately selected firmware programs, evaluated manually on a Raspberry Pi Pico with RP2040, using an oscilloscope for servo signals or a TTL adapter and terminal for UART output. They are not host unit tests and are not executed by cargo test or the normal development check or verify task. Cargo registers them as example targets with explicit paths in Cargo.toml.

## Setup

For the PIO tests, use unloaded GP0 and GP1 with no servos connected. Connect oscilloscope probes to these outputs and reference both channels to Pico GND. Use DC coupling to distinguish a persistent Low level from missing trigger activity. The current instrument is a Leader 1041 two-channel analog oscilloscope; a single startup pulse sequence may not be visible.

## UART connection and terminal

The UART tests use a Waveshare USB TO RS232/485/TTL adapter in **3.3 V TTL mode**. Use its TTL terminals:

| Pico | Adapter |
| --- | --- |
| GP20, physical pin 26, UART1 TX | TTL RXD |
| GND, for example physical pin 23 | TTL GND |

Leave the adapter TXD and power terminals disconnected. Power the Pico through USB. The UART tests do not start servo output; GP0 and GP1 remain unused by these tests.

Open HTerm at **115200 baud, 8 data bits, no parity, one stop bit, no flow control**. Lines end with LF (`\n`) only. Configure terminal display handling for LF as needed. GP21 is reserved for a possible future UART1 RX connection; the current tests are transmit-only.

## Build and convert

From the repository root:

```bash
python tools/dev.py hardware-build
```

This builds all registered hardware-test programs and converts each reported ELF executable to UF2. It requires the embedded Rust target and elf2uf2-rs. It does not flash the Pico or evaluate results. Files are generated under:

```text
firmware/pico/target/thumbv6m-none-eabi/debug/examples/
```

To build and convert just one program, run from firmware/pico:

```bash
cargo run --locked --example underflow_missing_counter --features hardware-tests
```

Transfer the selected UF2 using BOOTSEL and the RPI-RP2 drive as described in the [firmware guide](../../README.md). Each transfer replaces the currently installed firmware. Reboot or transfer a fresh image to begin another run. The normal demo remains available through python tools/dev.py uf2 from the repository root.

## Programs and manual checks

| Program | Procedure and expected result |
| --- | --- |
| fixed_plan | Observe continuous approximately 1-ms High pulses on GP0 and 2-ms High pulses on GP1, with synchronous rising edges and a nominal 20-ms period. LED phases are approximately 500 ms. Measure the period separately. |
| underflow_missing_entry | Initially observe the normal pulse sequence. At approximately 5 s, DMA rearming is paused and both outputs must settle Low. LED phases change to 100 ms when PIO IRQ flag 0 is set. After 10 s, servicing resumes but both outputs must remain Low. |
| underflow_missing_counter | The first transfer ends after the final mask, omitting its counter. PIO must force Low and latch its error flag. LED phases become 100 ms. Servicing starts after 1 s; outputs must remain Low. The initial one-off pulse sequence may be too brief to observe. |
| switching_signal_plans | Alternate GP0/GP1 between 1 ms/2 ms and 2 ms/1 ms approximately once per second. Rising edges remain synchronous; nominal period is 20 ms. An immediate second submission must return Busy. Normal LED phases are 500 ms, fault phases 100 ms; a failed assertion halts execution. |
| faulted_signal_output | Run normally for approximately 5 s, prepare a replacement, and stop DMA servicing. On underflow, assert Faulted instead of Busy, clear hardware IRQ flag 0, and assert that the driver remains faulted and rejects further plans. Resume service calls; outputs stay Low. LED phases become 100 ms after the assertions pass. |
| restart_signal_output | Run initial GP0/GP1 pulses for 5 s, induce real underflow, remain Low for 2 s, then restart with 2-ms/1-ms pulses. Reject restart while healthy and invalid period/output requests while faulted. LED phases are 100 ms while faulted, 500 ms otherwise. |
| restart_active_dma | After 5 s, inject a fault with PIO disabled and outputs Low; confirm a full FIFO and active DMA. Remain Low for 2 s with DMA still active, then restart with 2-ms/1-ms pulses. The successful restart must abort the still-active transfer. |
| restart_abort_timeout | After 5 s, inject a fault with active DMA, request restart with zero abort polling attempts, and assert DmaAbortTimeout. Keep outputs Low and the transfer owned for 2 s, then retry with the normal budget and observe 2-ms/1-ms pulses. UART reports the forced timeout and successful retry. |
| servo_bank_angles | Exercise calibrated complete and partial angle commands every 2 s. Observe GP0/GP1 widths 1500/1500, 1000/2000, 1500/2000, 1500/1000, unchanged after an invalid batch, then 1500/1500 us. Rising edges should remain synchronous. |
| uart_diagnostics | Receive three startup lines (INFO, DEBUG, ERROR) and then INFO Pico alive about once per second. The LED toggles with each alive message. Output is queued in a 128-byte logger and serviced with a 16-byte budget per loop. |
| uart_panic | Receive the buffered startup message. After approximately 5 s, the LED turns on and a deliberate panic prints its message and source location directly through UART1. The LED stays on and normal execution stops. |

In the original underflow and switching tests, fast blinking means the program observed PIO IRQ flag 0. In faulted_signal_output it means the fault-state assertions passed, including persistence after clearing that hardware flag. It is not a calibrated timing measurement. The fault tests establish persistent electrical Low behavior, not a safe mechanical posture or servo position feedback. These original fault tests do not perform recovery. The two restart programs exercise explicit recovery separately.

## Shared support and feature boundary

support/mod.rs initializes board clocks, GPIO ownership, and the hardware timer, and supplies UART1 and its peripheral clock to the UART tests. fixed_plan reuses the normal servo_demo. The underflow, switching, and fault-state tests share support/output.rs for the fixed two-output plan, static storage, and nonblocking LED indicator; each test keeps its own execution sequence.

The Pico hardware-tests feature forwards to servo-pio/hardware-tests. That feature exposes start_with_missing_final_counter and its truncation mode, plus dma_is_active and force_fault_with_active_dma for active-DMA fault injection, and restart_with_zero_abort_budget and has_dma_transfer for timeout-path verification. Normal start_fixed continues to transfer complete plans. The missing-entry test requires no driver fault injection: it deliberately pauses service calls. No production source edits are needed to select or repeat these experiments.

## Reported verification

On 2026-10-17 the owner reported:

- The fixed-plan demo produced approximately 1 ms and 2 ms High durations with synchronous rising edges.
- The separately selected missing-entry test behaved as described.
- The missing-counter test appeared to enter fault mode immediately; no startup edges were visible. This matches the expected millisecond-scale fault, but no precise fault latency was measured.
- The earlier full verification passed 121 host unit tests, and hardware-build created the UF2 files. The owner subsequently reported green CI for the hardware-test and task-splitting revisions.
- The two-buffer revision passed 128 unit tests and hardware-test builds. switching_signal_plans ran as expected with synchronous rising edges and alternating High durations. Continued execution also indicates that its Busy assertion did not halt the program.

The 128 unit-test total excludes these manually evaluated firmware programs. New exact period measurements, worst-case DMA service latency, 18-channel output, controlled recovery, and attached-servo operation have not been established. The owner subsequently reported green CI for two-buffer switching. Remote CI for the fault-state revision is pending.

## Persistent driver fault test

On 2026-10-17, the owner reported all requested checks passing and faulted_signal_output running as expected. The test first establishes a pending replacement and Busy, then verifies that submit_plan detects a real PIO underflow and returns Faulted before considering the occupied slot. It clears hardware flag 0 only after the driver has latched the fault. Subsequent status checks and submissions must still report faulted, and further servicing must not restart the signals.

The electrical Low response still comes from PIO. The driver observes it when is_faulted, submit_plan, or service reads the reserved flag; detection is not an asynchronous CPU interrupt. A flag cleared before observation cannot be discovered through that flag. The test does not perform recovery.

The original underflow, switching, and persistent-fault tests use panic_halt without UART assertion output. fixed_plan, uart_panic, all restart tests, and servo_bank_angles select the UART panic handler. The restart programs initialize UART1 and log state transitions; fixed_plan shares the normal demo logging. A failed assertion stops CPU progress and LED updates; the last LED level alone does not identify the failing assertion. PIO/DMA may continue until their data supply runs out. Observe a continuing blink pattern, not merely a particular static LED level.

## UART verification record

On 2026-10-17, the owner confirmed that uart_diagnostics delivered Info, Debug, and Error output to HTerm, including after the LF-only change. The owner then reported 145 passing tests in the full runner and confirmed that uart_panic emitted its message after approximately five seconds and left the LED permanently on.

The logging library contributes 17 unit tests to that total. These manual UART tests do not add to the count. See the [logging guide](../../logging/README.md) for buffer and panic contracts. The owner subsequently confirmed UART output alongside servo pulses in the normal demo and reported green CI for the earlier logging revision. The normal firmware now selects the UART panic handler. Panic before UART initialization and a stalled UART have not been separately tested on hardware.

## Controlled restart tests

Both programs use the UART wiring above and unloaded GP0/GP1. From the repository root, build all ELF/UF2 files with `python tools/dev.py hardware-build`. Upload either `restart_signal_output.uf2` or `restart_active_dma.uf2` from the generated examples directory.

Expected phases are initial 1-ms/2-ms pulses for approximately five seconds, both outputs Low for approximately two seconds, then 2-ms/1-ms pulses with synchronous rising edges. The LED blinks faster in the fault phase. UART reports state transitions and does not emit periodic alive messages. Assertions and expect failures use the UART panic handler.

`restart_signal_output` stops servicing DMA with a replacement still pending and waits for real underflow. It checks NotFaulted while healthy, Faulted on submission after underflow, and rejection of invalid restart periods/output sets. Servicing during the Low interval must not restart output. A successful explicit restart selects the supplied replacement and clears the driver fault.

`restart_active_dma` injects its fault instead of waiting for underflow. The hardware-tests hook disables PIO execution, forces Low, sets flag 0, safely retires the previous DMA transfer, and starts a full six-word transfer with no consuming state machine. The four-word FIFO must fill while DMA remains active. Assertions confirm active DMA after injection, after rejected restart requests, throughout the Low interval, and immediately before successful restart. This distinguishes the test from recovery after a transfer that may already have completed.

On 2026-10-17 the owner reported 149 passing unit tests and successful manual execution of both tests, including the signal restart and expected LED behavior. The active-DMA test's successful execution also confirms its DMA-status assertions passed. The manual programs are not included in the unit-test total. The timeout return path was subsequently tested with zero polling attempts as recorded below; exact latency and short-glitch exclusion remain unverified. CI for this recovery revision has not yet been reported.

## Forced abort-timeout test

`restart_abort_timeout` follows the active-DMA fault setup. A feature-gated wrapper calls the same restart implementation with zero permitted status polls. Plan validation and the real channel abort request still occur; the helper then returns Timeout without confirming completion. The test asserts DmaAbortTimeout, persistent driver fault, retained transfer ownership, and rejection of ordinary plan submissions.

For the next two seconds, service calls must not restart output and the transfer must remain owned. DMA may already have stopped asynchronously; BUSY is deliberately not required to remain set after the abort request. A normal restart with a fresh complete replacement must then succeed. Expected signals are five seconds of initial pulses, a Low interval, and restarted GP0/GP1 pulses of 2 ms/1 ms. The LED indicates fault and running phases, and UART reports the timeout and successful retry.

On 2026-10-27 the owner reported 169 passing unit tests, successful signal checks, and expected UART output for this program. This verifies the exhausted-budget return path and subsequent recovery, not a physically stuck DMA channel or the elapsed duration of a real abort timeout. The manual program does not add to unit-test counts. CI for this timeout revision has not yet been reported.

## Servo-bank angle test

`servo_bank_angles` exercises ServoBank -> ServoSignalOutput -> SwitchingOutput -> DMA/PIO on unloaded GP0/GP1. Both synthetic calibrations map -45/0/+45 degrees to 1000/1500/2000 us. The driver starts with a matching center plan; the bank then submits its initial complete center request. Every later angle change passes through the bank and adapter. The main loop calls bank.service and services the UART logger without blocking delays.

At two-second intervals, the program sends complete opposite-angle targets, changes only servo 0, changes only servo 1, rejects a batch containing +46 degrees, and resets both servos to center before repeating. Assertions check retained command values after partial and rejected requests. UART messages describe accepted or rejected commands, not physical activation acknowledgements.

On 2026-10-27 the owner reported 169 passing unit tests, a successful signal check, and subsequently green CI for the angle-test revision. This confirms the integrated command-to-signal path with synthetic calibration. Exact timing, brief-glitch exclusion, actual servo positions, and measured mechanical calibration remain unverified.

## Read-only calibration journal test

`flash_journal_read` checks the reserved 8-KiB region through FlashJournalReader -> JournalRead -> load -> find_latest for eighteen servo IDs. It also checks that read_slot rejects SLOT_COUNT as out of range. It uses the software CRC adapter and starts no servo output, flash programming or flash erase. Use the established UART1 wiring: GP20 to adapter TTL RXD, shared GND, 3.3-V logic, HTerm at 115200 baud/8N1/no flow control.

From the repository root:

```bash
python tools/dev.py hardware-build
```

After the build/conversion succeeds, hold BOOTSEL while connecting the Pico and copy `firmware/pico/target/thumbv6m-none-eabi/debug/examples/flash_journal_read.uf2` to RPI-RP2. This programs the test firmware; the running test itself does not write the journal. Prepare HTerm before startup because the result is emitted once.

Expected result: `INFO Journal read OK: no valid record` if no slot is valid, or `INFO Journal read OK: valid record found` if an existing record passes validation. Either successful read result sets the LED high. Invalid layout, failed bounds checking or load failure reports an error and leaves the LED low. A high LED indicates test completion, not configuration completeness or robot-operation permission. This test uses panic-halt; an unexpected panic may halt without a UART panic message.

On 2026-10-28 the owner reported `INFO Journal read OK: no valid record`, followed by 244 passing tests and all 13 checks passing. LED state was not separately reported. The region may contain erased or invalid data; absence of a valid record does not distinguish them. A known valid on-flash record and write/erase/persistence behavior remain untested. CI for this read-only integration is pending.
