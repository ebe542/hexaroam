#![no_std]
#![no_main]

mod panic_uart;
mod servo_demo;
mod uart_logging;

use rp_pico::entry;
use rp_pico::hal::{
    Timer,
    clocks::{Clock, init_clocks_and_plls},
    pac,
    sio::Sio,
    watchdog::Watchdog,
};

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

    servo_demo::run(
        peripherals.PIO0,
        peripherals.DMA,
        pins,
        &mut peripherals.RESETS,
        clocks.system_clock.freq().to_Hz(),
        timer,
        peripherals.UART1,
        clocks.peripheral_clock,
    )
}
