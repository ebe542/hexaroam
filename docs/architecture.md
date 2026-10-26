# Architecture

## Design principles

HexaRoam is a learning project. Implement the robot-specific layers directly, using documentation and existing implementations as references. General-purpose development boards and standard components are acceptable. ROS is an optional adapter, not a core dependency.

Hardware and software must support incremental development and changes to robot geometry.

## Layers

1. Behavior selects activities such as exploration, observation, and rest.
2. Navigation converts goals into desired body motion.
3. Motion control generates gait phases, foot trajectories, and joint targets.
4. Firmware validates and executes coordinated joint targets.
5. Board drivers generate servo signals and access peripherals.

Perception and state estimation supply observations to navigation and behavior. Diagnostics and recording span all layers. Future speech commands submit requests to the same command interface as manual control.

## Controller topology

Start with one Pico. The central Pico coordinates joints and communicates with a future Linux host. Add Pico modules as servo-output expanders only when needed; do not assume one controller per leg.

Expansion modules execute buffered targets, enforce local limits, monitor communication, and report status. Plan a shared time reference and future execution timestamps. The transport, clock synchronization method, timing tolerances, and module capacity remain open.

A module failure requires a robot-wide response as well as local handling. Define the response for each tested posture; disabling torque can cause collapse.

## Configuration

Keep geometry, joint-to-module mappings, output assignments, direction, offsets, and limits outside robot-specific algorithms. Firmware must still enforce validated local limits.

Distinguish commanded positions from measured positions. Do not report position feedback unless suitable sensors provide it.

## Hardware revisions

Every built revision should identify its mechanical files, BOM, wiring, robot configuration, and compatible firmware revision. Imported designs require source and license records; modifications require change notes.

## Initial verification

Develop parsing, limits, and motion calculations on the host where possible. Verify physical signal timing and behavior on real hardware. Simulation cannot establish electrical correctness.

## Servo output working design

See the [multi-channel servo output design](servo-output-design.md) for the single-state-machine PIO/DMA backend, whole-plan updates, buffer ownership, and underflow recovery. The current fixed-plan demo drives two outputs through one state machine and DMA. A separate SwitchingOutput driver and hardware test now exercise two-buffer plan replacement. SwitchingOutput now supports explicit restart with a validated complete plan after a latched fault. Separate hardware tests cover recovery after real underflow and aborting a still-active DMA transfer. The normal demo remains fixed-plan; explicit activation feedback, interrupt-based handover, and forced abort-timeout verification remain open.

## Transactional servo commands

ServoBank owns per-servo calibrations and last-accepted angle commands. Each nonempty request is validated in full, expanded with unchanged accepted targets, and converted to one complete pulse-width batch. The first nonempty request must initialize every servo. Bank state changes only after the PulseBankOutput backend accepts the complete batch. Busy or Faulted leaves the command records unchanged; acceptance does not establish activation or measured joint position.

The bank has no board mapping. The next adapter will map ServoId-ordered pulse widths to GPIOs and build a complete SignalPlan for SwitchingOutput. This adapter and the new bank's Pico integration remain pending. See the [servo-control guide](../firmware/servo-control/README.md#servo-bank).
