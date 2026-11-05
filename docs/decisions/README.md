# Design decisions

Record significant choices in numbered Markdown files: context, decision, alternatives, consequences, and verification.

Current direction: Rust firmware on general-purpose Pico boards; optional Pico output expansion; own robot-specific software; optional ROS integration. The current controller is an RP2040 Pico. The proposed operating interface uses UART with COBS commands and boot-only calibration from an externally programmed I2C EEPROM; the EEPROM component and hardware revision remain to be selected.

## Recorded decisions

- [0001: COBS command protocol](0001-cobs-command-protocol.md) - agreed direction with external read-only calibration EEPROM; remaining wire/timing details provisional; not implemented.
