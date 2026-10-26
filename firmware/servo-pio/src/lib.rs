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

// Temporary until the output driver uses pending-plan submission.
#[allow(dead_code)]
mod pending_signal_plan;

// Temporary until the switching output driver uses this source.
#[cfg(all(target_arch = "arm", target_os = "none"))]
#[allow(dead_code)]
mod signal_plan_dma_source;

#[cfg(all(target_arch = "arm", target_os = "none"))]
mod switching_output;

#[cfg(all(target_arch = "arm", target_os = "none"))]
pub use switching_output::{
    OutputRestartError, OutputSubmissionError, SwitchingOutput,
};

// Used by the controlled restart implementation in the next step.
#[cfg(all(target_arch = "arm", target_os = "none"))]
#[allow(dead_code)]
mod dma_abort;
