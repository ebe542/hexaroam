#![no_std]
#![no_main]

//! Manual hardware test: reject submissions after PIO underflow.
//!
//! First 5 seconds: normal fixed-plan output.
//! Then prepare a replacement and stop servicing DMA.
//! After underflow:
//! - submit_plan must return Faulted rather than Busy;
//! - clearing PIO IRQ flag 0 must not clear the driver fault;
//! - further service calls must not restart output.
//!
//! LED phases: 500 ms during setup, 100 ms after assertions pass.
//! A failed assertion halts execution.
//! Observe GP0/GP1 without connected servos.

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

    let replacement = SignalPlan::<3>::new(
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
    .expect("Valid replacement plan");

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
    .expect("Failed to start output");

    let started_us = fixture.timer.get_counter_low();
    let mut blinker = Blinker::new(fixture.led, started_us);

    let mut waiting_for_fault = false;
    let mut fault_wait_started_us = started_us;
    let mut assertions_passed = false;

    loop {
        let now_us = fixture.timer.get_counter_low();

        if !waiting_for_fault && !assertions_passed {
            if now_us.wrapping_sub(started_us) < 5_000_000 {
                output.service();
            } else {
                output
                    .submit_plan(&replacement)
                    .expect("Replacement must be accepted");

                assert_eq!(
                    output.submit_plan(&fixture.plan),
                    Err(OutputSubmissionError::Busy),
                );

                waiting_for_fault = true;
                fault_wait_started_us = now_us;
            }
        }

        if waiting_for_fault {
            // Deliberately do not call service: the ready plan
            // must remain pending while the current stream runs out.
            if fixture.pio.get_irq_raw() & 1 != 0 {
                // submit_plan itself must detect and latch the fault.
                assert_eq!(
                    output.submit_plan(&fixture.plan),
                    Err(OutputSubmissionError::Faulted),
                );
                assert!(output.is_faulted());

                fixture.pio.clear_irq(1 << 0);
                assert_eq!(fixture.pio.get_irq_raw() & 1, 0);

                assert!(output.is_faulted());
                assert_eq!(
                    output.submit_plan(&fixture.plan),
                    Err(OutputSubmissionError::Faulted),
                );

                waiting_for_fault = false;
                assertions_passed = true;
            } else {
                assert!(
                    now_us.wrapping_sub(fault_wait_started_us) < 1_000_000,
                    "Expected PIO underflow was not observed",
                );
            }
        }

        if assertions_passed {
            output.service();
            assert!(output.is_faulted());
        }

        let interval_us = if assertions_passed { 100_000 } else { 500_000 };

        blinker.update(now_us, interval_us);
    }
}
