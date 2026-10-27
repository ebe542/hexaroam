# Milestones

| Stage | Scope | Completion evidence |
| --- | --- | --- |
| 1 | One Pico, one servo, Rust | Explicit enable, bounded motion, verified signal timing |
| 2 | Joint module | Calibration, smooth transitions, coordinated outputs, rejected invalid commands |
| 3 | One leg | Forward/inverse kinematics and reachable foot targets |
| 4 | Optional expansion | Two modules execute coordinated targets; late commands and lost communication are handled |
| 5 | Whole robot | Documented assembly, calibration, stable supported commissioning and standing |
| 6 | Walking | Controlled walking and turning under manual commands |
| 7 | Sensors | Timestamped observations and demonstrated obstacle response |
| 8 | Navigation | A specified goal is reached in a controlled environment |
| 9 | Behavior and AI | Exploration, activity selection, recorded datasets, and evaluated custom models |
| Later | Audio | Fixed speech commands, then broader interaction |

Stages may overlap. Expansion is conditional on actual output needs.

## First milestone

Use the existing Raspberry Pi Pico (RP2040). Select a servo, document pinout and power wiring, complete the Rust embedded setup, and first verify compilation and USB programming with a blinking board LED. Then implement one bounded servo output. Test without a loaded leg and record the observed timing and behavior.

## Progress

2026-10-12: The owner confirmed the embedded Rust target, successful `cargo check` and `cargo build`, UF2 conversion, USB transfer, and a blinking onboard LED on the physical Pico under Windows. See the [firmware guide](../firmware/README.md) for reproduction steps and verification limits.

2026-10-13: The owner confirmed that the PIO firmware runs, the LED continues blinking, and a periodic pulse is visible at the output. Analog oscilloscope readings were approximately 19.8 ms period and 1.4 ms high time versus nominal 20 ms and 1.5 ms. Measurement uncertainty was not quantified; these observations establish signal generation, not exact timing accuracy. No servo was attached.

2026-10-14: The owner reported six successful host unit tests for the extracted no_std signal library and a successful Pico build. The integrated firmware ran on the Pico with apparently unchanged alternating 1000/1500/2000-us pulses. Limits are configured by the application, and the private PIO adapter handles encoding and FIFO access. Precise timing and initialization error paths remain unverified.

2026-10-14, angle integration: The owner reported a successful full local check (30 tests, formatting, and Pico build), successful UF2 upload, and expected firmware behavior without an attached servo. The application now commands synthetic -45/0/+45-degree targets through calibration, the signal layer, and PIO. Physical calibration and actual servo operation remain pending. CI for the preceding Servo wrapper was green; remote integration verification is pending.

This completes the setup, blink, adjustable-signal, and synthetic angle-command prerequisites, not the first servo milestone. Actual servo operation, power wiring, and calibration remain pending.

## Open decisions

- Required GPIO allocation for the selected Raspberry Pi Pico.
- Servo model and electrical requirements.
- One-controller capacity and signal-generation approach.
- Expansion transport and synchronization requirements.
- Power budget, distribution, and battery arrangement.
- Host environment and sensor selection.

## Two-channel PIO verification

2026-10-15: The owner reported a successful local check with 68 passing tests and physical execution with a blinking LED. PIO0 SM0 drives GP0 and SM1 drives GP1 using one shared eight-instruction program. The driver is generic over the PIO block, state machine index, and GPIO identifier; program installation is separate from channel initialization.

Both outputs were reported to follow the agreed opposing 1000/1500/2000-us and 2000/1500/1000-us sequences. Rising edges were not synchronized, consistent with separate channel starts. No new numerical timing measurements were reported. Physical servo operation and coordinated channel start remain pending. The owner subsequently reported a green GitHub Actions workflow for this two-channel revision.

## Validated servo bank

2026-10-16: The owner reported a successful full local check with 80 tests after adding two output-free angle validation tests and ten ServoBank tests. The bank validates all IDs and calibrated angle ranges before submitting any command. Validation failure produces no output calls. Submission errors stop further commands without retry or rollback; earlier accepted commands remain accepted.

This library step does not change the Pico demonstration. ServoBank integration, synchronized PIO startup, and simultaneous activation of later target updates remain pending. Remote CI for this library revision has not yet been reported.

## Fixed-plan PIO/DMA output

2026-10-17: The owner reported a full local check with 114 passing tests and successful execution of the fixed-plan DMA demo. One PIO0 state machine and DMA channel 0 drive GP0 and GP1. The observed High durations were approximately 1 ms and 2 ms, with synchronous rising edges. A static encoded plan repeats through polling-based DMA servicing; LED updates are nonblocking.

The nominal period is 20 ms, but no new period measurement was reported. Deliberate underflow, recovery, dynamic plan switching, 18-channel verification, and attached-servo operation remain pending. Remote CI for this integration has not yet been reported.

## Separate hardware-test programs

2026-10-17: The owner reported that the separately built missing-entry test behaved as described. In the missing-counter test, no startup edges were visible and the program entered fault mode immediately, consistent with its intended short startup sequence. Exact fault latency was not measured. The fixed-plan demo remains independent of these test programs.

The owner reported 121 passing unit tests and successful ELF/UF2 generation through the hardware-build task. Hardware test programs are manually evaluated and excluded from unit-test counts. Controlled fault recovery and dynamic plan exchange remain pending; remote CI for this revision has not yet been reported.

## Complete signal-plan replacement

2026-10-17: The owner reported 128 passing unit tests, successful builds, and expected operation of the separate switching_signal_plans hardware test. GP0 and GP1 alternate their High durations while rising edges remain synchronous. The test also asserts Busy for an immediate second submission; no assertion halt was reported.

The new output path uses two exclusive static RAM buffers and one DMA channel, selecting complete plans after DMA transfer completion while FIFO ordering preserves their sequence. The normal fixed-plan demo and underflow tests remain available. Exact update timing, short-glitch exclusion, active-plan acknowledgement, controlled recovery, hardware-fault submission handling, and 18-channel operation remain unverified. Remote CI for this revision has not yet been reported.

## Persistent output fault

2026-10-17: The owner reported all requested checks passing and expected operation of faulted_signal_output. The test confirms that a detected PIO underflow makes submit_plan return Faulted even with a pending replacement. Clearing the hardware IRQ flag does not clear the driver fault, and later servicing does not restart output. The host unit-test total remains 128; this is an additional manually evaluated hardware program.

Controlled recovery, independent stale-flag startup testing, and exact fault latency remain pending. The preceding switching revision passed CI; CI for this fault-state revision is pending.

## Buffered UART logging and bounded panic output

2026-10-17: The owner reported 17 passing logging unit tests and 145 passing tests in the full runner. The no_std logging library queues complete LF-terminated lines, rejects whole new messages when full, and services a nonblocking backend with a byte budget. A separate formatted writer bounds the total number of direct write attempts.

The owner confirmed Info, Debug, and Error output in HTerm from the UART1/GP20 hardware test. The separate uart_panic program emitted its message after approximately five seconds and left the LED permanently on. Its handler bypasses the runtime software queue and then holds the CPU in a permanent loop. The attempt budget is not a calibrated time limit.

See the [logging guide](../firmware/logging/README.md) for contracts and the [hardware-test guide](../firmware/pico/hardware_tests/README.md#uart-connection-and-terminal) for reproduction. The regular servo firmware still uses panic-halt; UART integration with running servo DMA and controlled output recovery remain pending. CI for this revision has not yet been reported.

## Controlled restart and active-DMA abort

2026-10-17: The owner reported 149 passing unit tests and successful manual execution of restart_signal_output and restart_active_dma. The first test exercises recovery after real PIO underflow, rejecting invalid restart requests while preserving the fault. The second injects a fault, confirms a full FIFO with DMA still active, and checks active status immediately before the successful restart. Both showed initial pulses, a Low interval, renewed output, and correct LED behavior.

Restart validates the supplied complete plan before hardware changes and retains DMA source ownership until a channel-specific abort confirms inactivity. RP2040-E13 handling includes BUSY polling and channel interrupt handling. A bounded polling failure leaves the driver faulted. The forced timeout path remains untested on hardware. The normal fixed-plan demo now runs UART logging alongside servo DMA and selects the bounded UART panic handler; the owner confirmed simultaneous output and pulses. CI for the recovery revision has not yet been reported.

## Transactional servo-bank commands

2026-10-26: The owner reported 16 passing bank tests and a full verification total of 155 passed, 0 failed, 0 skipped, and 0 filtered out. ServoBank now submits complete pulse-width batches through PulseBankOutput and commits angle commands only after backend acceptance. The first nonempty request initializes all servos; later requests can update a subset. Validation and backend errors preserve the accepted commands. This replaces the earlier sequential bank and its ten tests; the control package now has 39 tests.

The library remains independent of GPIOs and the Pico HAL. A signal-plan adapter and Pico angle-demo integration are the next steps. This host-tested change does not establish new physical signal behavior or servo calibration. CI for this revision has not yet been reported.

## Signal-plan adapter

2026-10-26: The owner reported 14 passing ServoSignalOutput tests and full verification with 169 passed, 0 failed, 0 skipped, and 0 filtered out. The adapter validates fixed GPIO mapping and capacity, converts a complete pulse batch to one SignalPlan, and forwards backend errors without partial acceptance. A bank-to-adapter test confirms that plan validation failure does not commit angle commands. The control package now has 53 tests.

SignalPlanOutput keeps the backend interface independent of the Pico. SwitchingOutput implements it, and bank.service delegates through the adapter to DMA servicing. The preceding bank revision passed CI. A separate Pico angle-command demo and CI for this adapter revision remain pending; prior pulse-only hardware tests do not establish this new integrated angle path.
