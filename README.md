# HexaRoam

A modular six-legged robot built to explore — a learning project in Rust, robotics, and AI.

## Status

Rust firmware for Raspberry Pi Pico (RP2040) blinks the onboard LED and generates adjustable PIO servo pulses on GP0. The project owner confirmed six host unit tests for shared signal logic, a successful embedded build, and physical output observed with an analog oscilloscope. Exact timing accuracy, operation with an attached servo, and autonomous functionality remain unverified.

## Goals

- Build servo control, communication, kinematics, gait generation, and behavior logic step by step.
- Start with one general-purpose Raspberry Pi Pico and Rust.
- Add Pico output expansion modules when needed.
- Adapt the mechanical reference design and document each built configuration.
- Develop autonomous exploration, interaction, and later custom AI models.
- Keep ROS integration optional.

## Repository layout

| Directory | Contents |
| --- | --- |
| docs/ | Architecture, milestones, and decisions |
| hardware/mechanics/ | CAD, print files, and assembly |
| hardware/electronics/ | Wiring, power distribution, and board connections |
| hardware/bom/ | Bills of materials |
| hardware/references/ | Versioned official datasheets and reference documents |
| firmware/ | Rust firmware for Pico controllers |
| software/ | Motion, sensors, navigation, and behavior |
| simulation/ | Robot models and simulation experiments |
| tools/ | Calibration and diagnostics |
| configs/ | Geometry, joint mapping, and calibration |

Start with [architecture](docs/architecture.md) and [milestones](docs/milestones.md).

## Hardware reference

The initial mechanical reference is [ggldnl/Hexapod-Hardware](https://github.com/ggldnl/Hexapod-Hardware), published under CERN-OHL-W-2.0. Upstream CAD and code are not bundled in this scaffold. Record provenance and applicable licenses when importing files; see [hardware attribution](hardware/ATTRIBUTION.md).

Official Pico and RP2040 PDFs, versions, licenses, and checksums are listed in [hardware references](hardware/references/README.md).

## Development

Rust is planned for firmware, communication, kinematics, and motion control. Python is planned for AI training, analysis, and experiments. The selected board is the existing Raspberry Pi Pico with RP2040, using the Rust target `thumbv6m-none-eabi`. Rust, rust-analyzer, and the embedded target are installed on the Windows development host. The blink and adjustable PIO pulse firmware has been built and run on the Pico. The communication bus and later Linux host environment remain open.

Follow the [development setup guide](docs/development-setup.md) for Windows with Git Bash or Ubuntu. It separates confirmed setup results from pending verification.

See the [firmware guide](firmware/README.md) for build, UF2 conversion, USB transfer, and the manual verification record. Ubuntu setup is documented but has not been verified for this project.

## Licensing

Original HexaRoam software and documentation are licensed under the [MIT License](LICENSE). Third-party materials, including imported or derived hardware designs, retain their applicable licenses; see [hardware attribution](hardware/ATTRIBUTION.md).
