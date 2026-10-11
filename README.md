# HexaRoam

A modular six-legged robot built to explore — a learning project in Rust, robotics, and AI.

## Status

Architecture and repository scaffold. No firmware or autonomous functionality is implemented yet.

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
| firmware/ | Rust firmware for Pico controllers |
| software/ | Motion, sensors, navigation, and behavior |
| simulation/ | Robot models and simulation experiments |
| tools/ | Calibration and diagnostics |
| configs/ | Geometry, joint mapping, and calibration |

Start with [architecture](docs/architecture.md) and [milestones](docs/milestones.md).

## Hardware reference

The initial mechanical reference is [ggldnl/Hexapod-Hardware](https://github.com/ggldnl/Hexapod-Hardware), published under CERN-OHL-W-2.0. Upstream CAD and code are not bundled in this scaffold. Record provenance and applicable licenses when importing files; see [hardware attribution](hardware/ATTRIBUTION.md).

## Development

Rust is planned for firmware, communication, kinematics, and motion control. Python is planned for AI training, analysis, and experiments. The exact Pico variant, toolchain, communication bus, and host environment are not selected yet.

No build or test commands are available at this stage. Each implemented component will document its setup and verification.

## Licensing

Original HexaRoam software and documentation are licensed under the [MIT License](LICENSE). Third-party materials, including imported or derived hardware designs, retain their applicable licenses; see [hardware attribution](hardware/ATTRIBUTION.md).
