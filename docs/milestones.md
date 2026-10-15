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

Both outputs were reported to follow the agreed opposing 1000/1500/2000-us and 2000/1500/1000-us sequences. Rising edges were not synchronized, consistent with separate channel starts. No new numerical timing measurements were reported. Physical servo operation and coordinated channel start remain pending. Remote CI for this revision has not yet been reported.
