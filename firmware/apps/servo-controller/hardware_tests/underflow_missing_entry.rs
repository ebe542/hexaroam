#![no_std]
#![no_main]

//! Manual hardware test: stop supplying complete signal plans.
//!
//! Setup: unloaded GP0/GP1, oscilloscope referenced to Pico GND.
//! First 5 seconds: 1 ms and 2 ms pulses with synchronous rising edges.
//! From 5 seconds: stop rearming DMA; both outputs must become Low.
//! From 10 seconds: resume servicing; outputs must remain Low.
//! LED: 500 ms phases normally, 100 ms phases after PIO fault flag 0.

mod support;

#[path = "support/output.rs"]
mod output_support;

use output_support::Blinker;
use panic_halt as _;
use rp_pico::entry;

#[entry]
fn main() -> ! {
    let fixture = output_support::prepare();

    let mut output = fixture
        .prepared
        .start_fixed(
            fixture.channel,
            fixture.buffer,
            &fixture.plan,
        )
        .expect("Failed to start output");

    let started_us = fixture.timer.get_counter_low();
    let mut blinker = Blinker::new(fixture.led, started_us);
    let mut resumed = false;

    loop {
        let now_us = fixture.timer.get_counter_low();
        let elapsed_us = now_us.wrapping_sub(started_us);

        if elapsed_us >= 10_000_000 {
            resumed = true;
        }

        if elapsed_us < 5_000_000 || resumed {
            output.service();
        }

        let faulted = fixture.pio.get_irq_raw() & 1 != 0;
        let interval_us = if faulted { 100_000 } else { 500_000 };

        blinker.update(now_us, interval_us);
    }
}
