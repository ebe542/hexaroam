#![no_std]
#![no_main]

//! Manual hardware test: continuous fixed-plan output.
//!
//! Setup: unloaded GP0 and GP1, oscilloscope referenced to Pico GND.
//! Expected: GP0 approximately 1 ms High, GP1 approximately 2 ms High,
//! synchronous rising edges, and a 20 ms period.
//! The onboard LED keeps blinking.
//!
//! Reuses the normal demo without changing its behavior.

mod support;

#[path = "../src/servo_demo.rs"]
mod servo_demo;

use panic_halt as _;
use rp_pico::entry;

#[entry]
fn main() -> ! {
    let mut board = support::initialize();

    servo_demo::run(
        board.pio,
        board.dma,
        board.pins,
        &mut board.resets,
        board.system_clock_hz,
        board.timer,
    )
}
