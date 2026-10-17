//! Shared board initialization for manually evaluated hardware tests.

use rp_pico::hal::{
    Timer,
    clocks::{Clock, init_clocks_and_plls},
    pac,
    sio::Sio,
    watchdog::Watchdog,
};

// Individual hardware tests use different subsets of these resources.
#[allow(dead_code)]
pub struct Board {
    pub pio: pac::PIO0,
    pub dma: pac::DMA,
    pub pins: rp_pico::Pins,
    pub resets: pac::RESETS,
    pub system_clock_hz: u32,
    pub timer: Timer,
    pub uart: pac::UART1,
    pub peripheral_clock: rp_pico::hal::clocks::PeripheralClock,
}

pub fn initialize() -> Board {
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

    Board {
        pio: peripherals.PIO0,
        dma: peripherals.DMA,
        pins,
        resets: peripherals.RESETS,
        system_clock_hz: clocks.system_clock.freq().to_Hz(),
        timer,
        uart: peripherals.UART1,
        peripheral_clock: clocks.peripheral_clock,
    }
}
