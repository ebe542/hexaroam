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
| uart_diagnostics | Receive three startup lines (INFO, DEBUG, ERROR) and then INFO Pico alive about once per second. The LED toggles with each alive message. Output is queued in a 128-byte logger and serviced with a 16-byte budget per loop. |
| uart_panic | Receive the buffered startup message. After approximately 5 s, the LED turns on and a deliberate panic prints its message and source location directly through UART1. The LED stays on and normal execution stops. |

In the original underflow and switching tests, fast blinking means the program observed PIO IRQ flag 0. In faulted_signal_output it means the fault-state assertions passed, including persistence after clearing that hardware flag. It is not a calibrated timing measurement. The fault tests establish persistent electrical Low behavior, not a safe mechanical posture or servo position feedback. They do not test controlled recovery, which is not implemented yet.

## Shared support and feature boundary

support/mod.rs initializes board clocks, GPIO ownership, and the hardware timer, and supplies UART1 and its peripheral clock to the UART tests. fixed_plan reuses the normal servo_demo. The underflow, switching, and fault-state tests share support/output.rs for the fixed two-output plan, static storage, and nonblocking LED indicator; each test keeps its own execution sequence.

The Pico hardware-tests feature forwards to servo-pio/hardware-tests. Only that feature exposes start_with_missing_final_counter and compiles its truncation mode. Normal start_fixed continues to transfer complete plans. The missing-entry test requires no driver fault injection: it deliberately pauses service calls. No production source edits are needed to select or repeat these experiments.

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

These PIO tests currently use panic_halt. Their assertion and expect messages are not sent to UART, USB, or stdout. Only uart_panic selects the new UART panic handler. A failed assertion stops CPU progress and LED updates; the last LED level alone does not identify the failing assertion. PIO/DMA may continue until their data supply runs out. Observe a continuing blink pattern, not merely a particular static LED level.

## UART verification record

On 2026-10-17, the owner confirmed that uart_diagnostics delivered Info, Debug, and Error output to HTerm, including after the LF-only change. The owner then reported 145 passing tests in the full runner and confirmed that uart_panic emitted its message after approximately five seconds and left the LED permanently on.

The logging library contributes 17 unit tests to that total. These manual UART tests do not add to the count. See the [logging guide](../../logging/README.md) for buffer and panic contracts. UART output combined with servo DMA, panic before UART initialization, and a stalled UART have not been separately tested on hardware. The normal firmware still uses panic-halt. CI for this logging revision has not yet been reported.
