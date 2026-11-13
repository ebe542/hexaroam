#![no_std]

//! Hardware-independent calibration-station commands and control.

pub mod command;
pub mod command_handler;
pub mod control;

#[cfg(all(target_arch = "arm", target_os = "none"))]
pub mod pio_pulse_backend;

pub mod frame_processor;
