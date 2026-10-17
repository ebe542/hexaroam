#![no_std]

//! PIO-specific encoding for servo signal plans.

// Temporary until the DMA-backed driver uses the encoder.
#[allow(dead_code)]
mod encoding;

// Temporary until the DMA-backed driver uses the program.
#[allow(dead_code)]
mod program;

// Temporary until the public output driver uses these resources.
#[cfg(all(target_arch = "arm", target_os = "none"))]
#[allow(dead_code)]
mod output;

// Temporary until the DMA-backed driver uses the internal methods.
#[allow(dead_code)]
mod buffer;

pub use buffer::OutputBuffer;

#[cfg(all(target_arch = "arm", target_os = "none"))]
pub use output::{
    FixedOutput, OutputInitError, OutputStartError, PreparedOutput,
};
