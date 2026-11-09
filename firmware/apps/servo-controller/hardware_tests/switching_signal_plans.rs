#![no_std]
#![no_main]

//! Manual hardware test: replace complete signal plans.
//!
//! Setup: unloaded GP0/GP1, oscilloscope referenced to Pico GND.
//! Plan A: GP0 1 ms High, GP1 2 ms High.
//! Plan B: GP0 2 ms High, GP1 1 ms High.
//! Alternate approximately once per second.
//! Rising edges must remain synchronous; nominal period is 20 ms.
//!
//! An immediate second submission must return Busy.
//! LED phases: 500 ms normally, 100 ms on PIO fault.
//! LED stops on an assertion failure.

mod support;

#[path = "support/output.rs"]
mod output_support;

use hexaroam_servo_pio::{
    OutputBuffer, OutputSubmissionError, SwitchingOutput,
};
use hexaroam_servo_signal::{PulseLimits, PulseTarget, SignalPlan};
use output_support::Blinker;
use panic_halt as _;
use rp_pico::entry;

#[entry]
fn main() -> ! {
    let fixture = output_support::prepare();

    let reversed_plan = SignalPlan::<3>::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: 2_000,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: 1_000,
            },
        ],
        &PulseLimits::new(1_000, 2_000).expect("Valid test limits"),
        20_000,
    )
    .expect("Valid reversed plan");

    let second_buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("Second buffer already allocated");

    let mut output = SwitchingOutput::start(
        fixture.prepared,
        fixture.channel,
        fixture.buffer,
        second_buffer,
        &fixture.plan,
    )
    .expect("Failed to start switching output");

    let started_us = fixture.timer.get_counter_low();
    let mut blinker = Blinker::new(fixture.led, started_us);
    let mut last_submission_us = started_us;
    let mut submit_reversed = true;

    loop {
        output.service();

        let now_us = fixture.timer.get_counter_low();
        let faulted = fixture.pio.get_irq_raw() & 1 != 0;

        if !faulted && now_us.wrapping_sub(last_submission_us) >= 1_000_000 {
            let (next_plan, other_plan) = if submit_reversed {
                (&reversed_plan, &fixture.plan)
            } else {
                (&fixture.plan, &reversed_plan)
            };

            match output.submit_plan(next_plan) {
                Ok(()) => {
                    // Without servicing in between, the prepared
                    // buffer must reject another submission.
                    assert_eq!(
                        output.submit_plan(other_plan),
                        Err(OutputSubmissionError::Busy),
                    );

                    last_submission_us = now_us;
                    submit_reversed = !submit_reversed;
                }
                Err(OutputSubmissionError::Busy) => {
                    // Retry the same desired plan on the next pass.
                }
                Err(error) => {
                    panic!(
                        "Unexpected submission error: {:?}",
                        error
                    );
                }
            }
        }

        let interval_us = if faulted { 100_000 } else { 500_000 };
        blinker.update(now_us, interval_us);
    }
}
