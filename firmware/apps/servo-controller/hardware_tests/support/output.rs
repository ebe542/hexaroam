//! Shared resources for the two-channel output hardware tests.

use embedded_hal::digital::OutputPin;
use hexaroam_servo_pio::{OutputBuffer, PreparedOutput};
use hexaroam_servo_signal::{PulseLimits, PulseTarget, SignalPlan};
use rp_pico::hal::{
    Timer,
    dma::{CH0, Channel, DMAExt},
    gpio::{FunctionPio0, FunctionSioOutput, Pin, PullNone, bank0::Gpio25},
    pac::PIO0,
    pio::{PIO, PIOExt, SM0},
};

type TestLed = Pin<Gpio25, FunctionSioOutput, PullNone>;

pub struct Fixture {
    pub prepared: PreparedOutput<PIO0, SM0, 2>,
    pub channel: Channel<CH0>,
    pub buffer: &'static mut OutputBuffer<6>,
    pub plan: SignalPlan<3>,
    pub pio: PIO<PIO0>,
    pub timer: Timer,
    pub led: TestLed,
}

pub fn prepare() -> Fixture {
    let mut board = crate::support::initialize();

    let (mut pio, sm0, _, _, _) = board.pio.split(&mut board.resets);

    let dma = board.dma.split(&mut board.resets);

    let pins = [
        board
            .pins
            .gpio0
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
        board
            .pins
            .gpio1
            .into_function::<FunctionPio0>()
            .into_pull_type::<PullNone>()
            .into_dyn_pin(),
    ];

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
        &PulseLimits::new(1_000, 2_000).expect("Valid test limits"),
        20_000,
    )
    .expect("Valid test plan");

    let buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("Output buffer already allocated");

    pio.clear_irq(1);

    let prepared = PreparedOutput::new(
        &mut pio,
        sm0,
        pins,
        board.system_clock_hz,
    )
    .expect("Failed to prepare outputs");

    Fixture {
        prepared,
        channel: dma.ch0,
        buffer,
        plan,
        pio,
        timer: board.timer,
        led: board
            .pins
            .led
            .into_push_pull_output()
            .into_pull_type::<PullNone>(),
    }
}

pub struct Blinker {
    led: TestLed,
    last_toggle_us: u32,
    high: bool,
}

impl Blinker {
    pub fn new(mut led: TestLed, now_us: u32) -> Self {
        led.set_low()
            .expect("Failed to initialize LED");

        Self {
            led,
            last_toggle_us: now_us,
            high: false,
        }
    }

    pub fn update(&mut self, now_us: u32, interval_us: u32) {
        if now_us.wrapping_sub(self.last_toggle_us) < interval_us {
            return;
        }

        self.last_toggle_us = now_us;
        self.high = !self.high;

        if self.high {
            self.led
                .set_high()
                .expect("Failed to turn LED on");
        } else {
            self.led
                .set_low()
                .expect("Failed to turn LED off");
        }
    }
}
