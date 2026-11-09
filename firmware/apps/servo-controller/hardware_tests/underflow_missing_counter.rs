#![no_std]
#![no_main]

//! Manual hardware test: final GPIO mask without its delay counter.
//!
//! Setup: unloaded GP0/GP1, oscilloscope referenced to Pico GND.
//! The first transfer omits the final counter.
//! Both outputs must become Low and PIO fault flag 0 must be set.
//! The LED then blinks with 100 ms phases.
//! After one second, normal servicing resumes.
//! Outputs must remain Low; servicing must not restart faulted PIO.

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
        .start_with_missing_final_counter(
            fixture.channel,
            fixture.buffer,
            &fixture.plan,
        )
        .expect("Failed to start fault-injection test");

    let started_us = fixture.timer.get_counter_low();
    let mut blinker = Blinker::new(fixture.led, started_us);
    let mut servicing_enabled = false;

    loop {
        let now_us = fixture.timer.get_counter_low();

        if now_us.wrapping_sub(started_us) >= 1_000_000 {
            servicing_enabled = true;
        }

        // Do not let another transfer supply the missing word
        // before the first transfer reaches its intended fault.
        if servicing_enabled {
            output.service();
        }

        let faulted = fixture.pio.get_irq_raw() & 1 != 0;
        let interval_us = if faulted { 100_000 } else { 500_000 };

        blinker.update(now_us, interval_us);
    }
}
