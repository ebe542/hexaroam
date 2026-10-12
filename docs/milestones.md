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

## Open decisions

- Required GPIO allocation for the selected Raspberry Pi Pico.
- Servo model and electrical requirements.
- One-controller capacity and signal-generation approach.
- Expansion transport and synchronization requirements.
- Power budget, distribution, and battery arrangement.
- Host environment and sensor selection.
