#![no_std]
#![no_main]

//! Manual COBS command-protocol test with real two-channel PIO output.
//!
//! Protocol UART0, 115200 baud, 8N1:
//! - GP16 TX -> adapter RXD
//! - GP17 RX <- adapter TXD
//! - Common GND, 3.3 V TTL, no flow control
//!
//! Debug UART1 TX: GP20, 115200 baud, 8N1.
//! Observe unloaded GP0 and GP1; do not connect servos.
//!
//! Synthetic calibration for both channels:
//! -45/0/+45 degrees -> 1000/1500/2000 us.
//! No EEPROM or calibration flash access.
//!
//! After boot, output is inactive and Status is 1.
//! Enable changes Status to 3 without generating pulses.
//! A full initial target set starts output; Status becomes 7.
//! Successful disable stops output and returns Status to 1.
//! LED blinking indicates main-loop activity, not output state.

mod support;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

#[path = "../src/uart_transport.rs"]
mod uart_transport;

use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_control::{
    Angle, Calibration, ServoBank, ServoSignalOutput,
};
use hexaroam_servo_pio::{OutputBuffer, PreparedOutput, SwitchingOutput};
use hexaroam_servo_registers::{
    command_processor::{CommandProcessingError, CommandProcessor},
    command_service::CommandService,
};
use hexaroam_servo_signal::{PulseLimits, SignalOutputState};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};
use rp_pico::{
    entry,
    hal::{
        clocks::Clock,
        dma::DMAExt,
        gpio::{FunctionPio0, FunctionUart, PullNone},
        pio::PIOExt,
        uart::{Pins, UartConfig, UartPeripheral},
    },
};
use uart_logging::UartLogOutput;
use uart_transport::{UartTransport, UartTransportError};

// Manual-test timing, not a production communication guarantee.
const RECEIVE_IDLE_TIMEOUT_US: u32 = 100_000;
const RECEIVE_BUDGET: usize = 32;
const TRANSMIT_BUDGET: usize = 32;

fn checksum(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

#[entry]
fn main() -> ! {
    let mut board = support::initialize();

    let debug_pins = Pins::default().tx(board
        .pins
        .gpio20
        .into_function::<FunctionUart>());

    let debug_uart = UartPeripheral::new(
        board.uart,
        debug_pins,
        &mut board.resets,
    )
    .enable(
        UartConfig::default(),
        board.peripheral_clock.freq(),
    )
    .expect("Failed to initialize debug UART");

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(debug_uart));

    let protocol_pins = Pins::default()
        .tx(board
            .pins
            .gpio16
            .into_function::<FunctionUart>())
        .rx(board
            .pins
            .gpio17
            .into_function::<FunctionUart>());

    let protocol_uart = UartPeripheral::new(
        board.protocol_uart,
        protocol_pins,
        &mut board.resets,
    )
    .enable(
        UartConfig::default(),
        board.peripheral_clock.freq(),
    )
    .expect("Failed to initialize protocol UART");

    let mut transport = UartTransport::new(protocol_uart);

    let calibration = Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .expect("Valid synthetic calibration");

    let limits = PulseLimits::new(1_000, 2_000).expect("Valid pulse limits");

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

    let first_buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("First buffer already allocated");

    let second_buffer = cortex_m::singleton!(
        : OutputBuffer<6> = OutputBuffer::new()
    )
    .expect("Second buffer already allocated");

    pio.clear_irq(1);

    let prepared = PreparedOutput::new(
        &mut pio,
        sm0,
        pins,
        board.system_clock_hz,
    )
    .expect("Failed to prepare outputs");

    let mut output = SwitchingOutput::new(
        prepared,
        dma.ch0,
        first_buffer,
        second_buffer,
        20_000,
    )
    .expect("Failed to initialize inactive output");

    assert_eq!(
        output.state(),
        SignalOutputState::Inactive
    );
    assert!(!output.dma_is_active());
    assert!(!output.is_faulted());

    let adapter =
        ServoSignalOutput::<_, 2, 3>::new(output, [0, 1], limits, 20_000)
            .expect("Valid signal adapter");

    let bank = ServoBank::new([calibration; 2], adapter);
    let service = CommandService::<_, 2>::new(bank, true);
    let mut processor = CommandProcessor::new(service);

    logger
        .log(
            LogLevel::Info,
            "UART COBS commands ready",
        )
        .expect("Ready message must fit");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut led_high = false;
    let mut last_toggle_us = board.timer.get_counter_low();
    let mut last_receive_us = last_toggle_us;
    let mut receive_timeout_armed = false;

    loop {
        match processor.poll(
            &mut transport,
            RECEIVE_BUDGET,
            TRANSMIT_BUDGET,
            &mut checksum,
        ) {
            Ok(progress) => {
                let now_us = board.timer.get_counter_low();

                if progress.received != 0 {
                    last_receive_us = now_us;
                    receive_timeout_armed = true;
                }

                // Budget exhaustion is not evidence of an idle UART.
                if progress.receive_empty
                    && receive_timeout_armed
                    && now_us.wrapping_sub(last_receive_us)
                        >= RECEIVE_IDLE_TIMEOUT_US
                {
                    processor.on_receive_timeout();
                    receive_timeout_armed = false;
                }
            }

            Err(CommandProcessingError::Receive(error)) => {
                // The HAL consumed an erroneous receive entry.
                // Discard the partial frame and await a new delimiter.
                processor.on_receive_timeout();
                receive_timeout_armed = false;

                let message = match error {
                    UartTransportError::Overrun => "Protocol UART RX overrun",
                    UartTransportError::Break => "Protocol UART RX break",
                    UartTransportError::Parity => {
                        "Protocol UART RX parity error"
                    }
                    UartTransportError::Framing => {
                        "Protocol UART RX framing error"
                    }
                };

                let _ = logger.log(LogLevel::Error, message);
            }

            Err(error) => {
                panic!("Command processing failed: {:?}", error);
            }
        }

        logger
            .service(16)
            .expect("Debug UART output is infallible");

        let now_us = board.timer.get_counter_low();

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
