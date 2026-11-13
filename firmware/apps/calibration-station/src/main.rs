#![no_std]
#![no_main]

mod app;
mod panic_uart;
mod uart_logging;
mod uart_transport;

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

    app::run(app::Resources {
        pins,
        resets: peripherals.RESETS,
        pio: peripherals.PIO0,
        dma: peripherals.DMA,
        protocol_uart: peripherals.UART0,
        debug_uart: peripherals.UART1,
        peripheral_clock: clocks.peripheral_clock,
        system_clock_hz: clocks.system_clock.freq().to_Hz(),
        timer,
    })
}
