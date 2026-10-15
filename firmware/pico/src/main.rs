#![no_std]
#![no_main]
mod drivers;

use drivers::servo_pio::ServoPio;
use embedded_hal::digital::OutputPin;
use hexaroam_servo_control::{Angle, Calibration, Servo};
use hexaroam_servo_signal::PulseLimits;
use panic_halt as _;
use rp_pico::entry;
use rp_pico::hal::sio::Sio;
use rp_pico::hal::{
    clocks::{Clock, init_clocks_and_plls},
    pac,
    watchdog::Watchdog,
};
use rp_pico::hal::{
    gpio::{FunctionPio0, PullNone},
    pio::PIOExt,
};

#[entry]
fn main() -> ! {
    let mut peripherals =
        pac::Peripherals::take().expect("RP2040 peripherals already taken");

    let core =
        pac::CorePeripherals::take().expect("Core peripherals already taken");

    let mut watchdog = Watchdog::new(peripherals.WATCHDOG);

    let clocks = init_clocks_and_plls(
        rp_pico::XOSC_CRYSTAL_FREQ,
        peripherals.XOSC,
        peripherals.CLOCKS,
        peripherals.PLL_SYS,
        peripherals.PLL_USB,
        &mut peripherals.RESETS,
        &mut watchdog,
    )
    .ok()
    .expect("Clock initialization failed");

    let mut delay = cortex_m::delay::Delay::new(
        core.SYST,
        clocks.system_clock.freq().to_Hz(),
    );

    let sio = Sio::new(peripherals.SIO);

    let pins = rp_pico::Pins::new(
        peripherals.IO_BANK0,
        peripherals.PADS_BANK0,
        sio.gpio_bank0,
        &mut peripherals.RESETS,
    );

    let (mut pio, sm0, _, _, _) = peripherals
        .PIO0
        .split(&mut peripherals.RESETS);

    let servo_pin = pins
        .gpio0
        .into_function::<FunctionPio0>()
        .into_pull_type::<PullNone>();

    let pulse_limits =
        PulseLimits::new(1_000, 2_000).expect("Invalid test pulse limits");

    let pulse_output = ServoPio::new(
        &mut pio,
        sm0,
        servo_pin,
        clocks.system_clock.freq().to_Hz(),
        pulse_limits,
        1_500,
    )
    .expect("Failed to initialize servo output");

    // Synthetic calibration for signal testing without a servo.
    let calibration = Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .expect("Invalid test calibration");

    let mut servo = Servo::new(calibration, pulse_output);

    let mut led = pins.led.into_push_pull_output();
    let target_angles = [
        Angle::from_centidegrees(-4_500),
        Angle::from_centidegrees(0),
        Angle::from_centidegrees(4_500),
    ];

    let mut target_index = 0;

    loop {
        led.set_high()
            .expect("Failed to turn LED on");
        delay.delay_ms(500_u32);

        led.set_low()
            .expect("Failed to turn LED off");
        delay.delay_ms(500_u32);

        servo
            .set_angle(target_angles[target_index])
            .expect("Failed to submit angle command");

        target_index = (target_index + 1) % target_angles.len();
    }
}
