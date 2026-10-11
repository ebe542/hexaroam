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

Select a Pico variant and servo, document pinout and power wiring, establish the Rust toolchain, and implement one bounded output. Test without a loaded leg and record the observed timing and behavior.

## Open decisions

- Exact Pico variant and required GPIO allocation.
- Servo model and electrical requirements.
- One-controller capacity and signal-generation approach.
- Expansion transport and synchronization requirements.
- Power budget, distribution, and battery arrangement.
- Host environment and sensor selection.
