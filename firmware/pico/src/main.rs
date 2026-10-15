#![no_std]
#![no_main]
mod drivers;

use drivers::servo_pio::{ServoPio, install_program};
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

    let (mut pio, sm0, sm1, _, _) = peripherals
        .PIO0
        .split(&mut peripherals.RESETS);

    let servo_pin = pins
        .gpio0
        .into_function::<FunctionPio0>()
        .into_pull_type::<PullNone>();

    let second_servo_pin = pins
        .gpio1
        .into_function::<FunctionPio0>()
        .into_pull_type::<PullNone>();

    let pulse_limits =
        PulseLimits::new(1_000, 2_000).expect("Invalid test pulse limits");

    let servo_program = install_program(&mut pio);

    // SAFETY: The program remains installed for the entire firmware runtime.
    // Neither handle is used to uninstall it.
    let second_servo_program = unsafe { servo_program.share() };

    let pulse_output = ServoPio::new(
        servo_program,
        sm0,
        servo_pin,
        clocks.system_clock.freq().to_Hz(),
        pulse_limits,
        1_500,
    )
    .expect("Failed to initialize servo output");

    let second_pulse_output = ServoPio::new(
        second_servo_program,
        sm1,
        second_servo_pin,
        clocks.system_clock.freq().to_Hz(),
        PulseLimits::new(1_000, 2_000)
            .expect("Invalid second test pulse limits"),
        1_500,
    )
    .expect("Failed to initialize second servo output");

    // Synthetic calibration for signal testing without a servo.
    let calibration = Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .expect("Invalid test calibration");

    // Synthetic calibration for signal testing without a servo.
    let second_calibration = Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .expect("Invalid second test calibration");

    let mut servo = Servo::new(calibration, pulse_output);
    let mut second_servo = Servo::new(second_calibration, second_pulse_output);

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

        let second_target_index = target_angles.len() - 1 - target_index;

        second_servo
            .set_angle(target_angles[second_target_index])
            .expect("Failed to submit second angle command");

        target_index = (target_index + 1) % target_angles.len();
    }
}
