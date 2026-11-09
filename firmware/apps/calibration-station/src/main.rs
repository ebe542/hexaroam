#![no_std]
#![no_main]

mod panic_uart;
mod uart_logging;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use rp_pico::entry;
use rp_pico::hal::{
    Timer,
    clocks::{Clock, init_clocks_and_plls},
    gpio::FunctionUart,
    pac,
    sio::Sio,
    uart::{Pins, UartConfig, UartPeripheral},
    watchdog::Watchdog,
};

use uart_logging::UartLogOutput;

#[entry]
fn main() -> ! {
    let mut peripherals =
        pac::Peripherals::take().expect("RP2040 peripherals already taken");

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

    let sio = Sio::new(peripherals.SIO);

    let pins = rp_pico::Pins::new(
        peripherals.IO_BANK0,
        peripherals.PADS_BANK0,
        sio.gpio_bank0,
        &mut peripherals.RESETS,
    );

    let timer = Timer::new(
        peripherals.TIMER,
        &mut peripherals.RESETS,
        &clocks,
    );

    let uart_pins = Pins::default().tx(pins
        .gpio20
        .into_function::<FunctionUart>());

    let uart = UartPeripheral::new(
        peripherals.UART1,
        uart_pins,
        &mut peripherals.RESETS,
    )
    .enable(
        UartConfig::default(),
        clocks.peripheral_clock.freq(),
    )
    .expect("Failed to initialize logging UART");

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(uart));

    let mut led = pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    logger
        .log(
            LogLevel::Info,
            "Calibration station ready",
        )
        .expect("Startup message must fit");

    let mut led_high = false;
    let mut last_toggle_us = timer.get_counter_low();

    loop {
        logger
            .service(16)
            .expect("UART output is infallible");

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
