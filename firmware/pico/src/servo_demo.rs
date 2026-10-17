//! Fixed two-channel signal test using one PIO state machine and DMA.

use embedded_hal::digital::OutputPin;
use hexaroam_servo_pio::{OutputBuffer, PreparedOutput};
use hexaroam_servo_signal::{PulseLimits, PulseTarget, SignalPlan};
use rp_pico::hal::{
    Timer,
    dma::DMAExt,
    gpio::{FunctionPio0, PullNone},
    pac,
    pio::PIOExt,
};

pub fn run(
    pio_peripheral: pac::PIO0,
    dma_peripheral: pac::DMA,
    pins: rp_pico::Pins,
    resets: &mut pac::RESETS,
    system_clock_hz: u32,
    timer: Timer,
) -> ! {
    let (mut pio, sm0, _, _, _) = pio_peripheral.split(resets);
    let dma = dma_peripheral.split(resets);

    let output_pins = [
        pins.gpio0
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
        pins.gpio1
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
    ];

    let limits = PulseLimits::new(1_000, 2_000).expect("Invalid test limits");

    let plan = SignalPlan::<3>::new(
        &[
            PulseTarget {
                gpio: 0,
                pulse_width_us: 1_000,
            },
            PulseTarget {
                gpio: 1,
                pulse_width_us: 2_000,
            },
        ],
        &limits,
        20_000,
    )
    .expect("Failed to construct test plan");

    // Allocate once in static RAM. The driver freezes the payload
    // before handing a static read-only slice to DMA.
    let buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("Output buffer already allocated");

    // IRQ flag 0 is reserved for this output's underflow indication.
    pio.clear_irq(1);

    let prepared = PreparedOutput::new(
        &mut pio,
        sm0,
        output_pins,
        system_clock_hz,
    )
    .expect("Failed to prepare servo outputs");

    let mut output = prepared
        .start_fixed(dma.ch0, buffer, &plan)
        .expect("Failed to start fixed output");

    let mut led = pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut led_high = false;
    let mut last_toggle_us = timer.get_counter_low();

    loop {
        output.service();

        let now_us = timer.get_counter_low();

        if now_us.wrapping_sub(last_toggle_us) >= 500_000 {
            last_toggle_us = now_us;
            led_high = !led_high;

            if led_high {
                led.set_high()
                    .expect("Failed to turn LED on");
            } else {
                led.set_low()
                    .expect("Failed to turn LED off");
            }
        }
    }
}
