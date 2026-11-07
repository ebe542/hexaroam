#![no_std]
#![no_main]

//! Manual UART throughput benchmark with 18 synthetic servo calibrations.
//!
//! Connect a 3.3 V TTL adapter:
//! - GP16 / UART0 TX -> adapter RXD
//! - GP17 / UART0 RX <- adapter TXD
//! - Pico GND       -> adapter GND
//!
//! Protocol UART: jumper-selected baud, 8N1, no flow control.
//! GP22/GP26 to GND: open/open = 115200,
//! connected/open = 128000, open/connected = 256000.
//! Both connected is invalid. Set jumpers before powering the board.
//! Selection is sampled only at startup; changes require a restart.
//! Pico physical pins: GP22 = 29, GND = 30, GP26 = 31.
//! Debug UART1 TX: GP20, always 115200 baud.
//!
//! COBS command protocol with CRC32.
//! Status after boot: 1, calibrated and disabled.
//! The PC enables output and supplies all 18 initial targets.
//! RUNNING represents the simulated pulse sink lifecycle only.
//! No physical servo pulses are generated.
//! Incoming ACK/NACK and corrupted frames receive no reply.
//! Flash and EEPROM are not accessed.
//! Handled requests receive ACK/NACK; no unsolicited protocol messages.

mod support;

#[path = "support/discard_pulse_output.rs"]
mod discard_pulse_output;

#[path = "../src/panic_uart.rs"]
mod panic_uart;

#[path = "../src/uart_logging.rs"]
mod uart_logging;

#[path = "../src/uart_transport.rs"]
mod uart_transport;

#[path = "support/test_message.rs"]
mod test_message;

use test_message::log_message;

use discard_pulse_output::DiscardPulseOutput;
use embedded_hal::digital::{InputPin, OutputPin};
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_control::{Angle, Calibration, ServoBank};
use hexaroam_servo_registers::{
    command_processor::{CommandProcessingError, CommandProcessor},
    command_service::CommandService,
};
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};
use rp_pico::{
    entry,
    hal::{
        clocks::Clock,
        fugit::RateExtU32,
        gpio::FunctionUart,
        uart::{Pins, UartConfig, UartPeripheral},
    },
};
use uart_logging::UartLogOutput;
use uart_transport::UartTransport;

// Initial manual-test value, not a production timing guarantee.
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

    // Benchmark-only selection. Internal pull-ups make open inputs High.
    // These pins remain inputs; the benchmark generates no servo signals.
    let mut baud_a = board.pins.gpio22.into_pull_up_input();
    let mut baud_b = board.pins.gpio26.into_pull_up_input();

    // Allow input levels to settle after enabling the pull-ups.
    let settle_started = board.timer.get_counter_low();
    while board
        .timer
        .get_counter_low()
        .wrapping_sub(settle_started)
        < 1_000
    {
        core::hint::spin_loop();
    }

    let selection = (
        baud_a
            .is_low()
            .expect("Failed to read GP22"),
        baud_b
            .is_low()
            .expect("Failed to read GP26"),
    );

    let (baudrate, baud_message) = match selection {
        (false, false) => (115_200_u32, "Benchmark baud: 115200"),
        (true, false) => (128_000_u32, "Benchmark baud: 128000"),
        (false, true) => (256_000_u32, "Benchmark baud: 256000"),
        (true, true) => {
            panic!("Invalid baud selection: both jumpers installed");
        }
    };

    logger
        .log(LogLevel::Info, baud_message)
        .expect("Baud message must fit");

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
        UartConfig::new(
            baudrate.Hz(),
            rp_pico::hal::uart::DataBits::Eight,
            None,
            rp_pico::hal::uart::StopBits::One,
        ),
        board.peripheral_clock.freq(),
    )
    .expect("Failed to initialize protocol UART");

    let mut transport = UartTransport::new(protocol_uart);

    // Synthetic calibration for protocol benchmarking only.
    // No calibration data is read from or written to flash.
    let calibration = Calibration::new(
        Angle::from_centidegrees(-4_500),
        1_000,
        Angle::from_centidegrees(4_500),
        2_000,
    )
    .expect("Valid synthetic calibration");

    let bank = ServoBank::new(
        [calibration; 18],
        DiscardPulseOutput::new(),
    );

    let service = CommandService::<_, 18>::new(bank, true);
    let mut processor = CommandProcessor::new(service);

    logger
        .log(
            LogLevel::Info,
            "COBS benchmark ready: 18 servos, no physical output",
        )
        .expect("Test message must fit");

    let mut led = board.pins.led.into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    let mut led_high = false;
    let mut last_toggle_us = board.timer.get_counter_low();
    let mut last_receive_us = last_toggle_us;
    let mut receive_timeout_armed = false;
    let mut received_bytes = 0_u64;
    let mut processed_requests = 0_u64;
    let mut transmitted_bytes = 0_u64;
    let mut receive_errors = 0_u64;
    let mut diagnostic_pending = false;
    let mut discarded_events = 0_u64;

    loop {
        match processor.poll(
            &mut transport,
            RECEIVE_BUDGET,
            TRANSMIT_BUDGET,
            &mut checksum,
        ) {
            Ok(progress) => {
                received_bytes += progress.received as u64;
                processed_requests += progress.processed as u64;
                transmitted_bytes += progress.transmitted as u64;
                discarded_events += progress.discarded as u64;

                if progress.received != 0
                    || progress.processed != 0
                    || progress.transmitted != 0
                {
                    diagnostic_pending = true;
                }

                let now_us = board.timer.get_counter_low();

                if progress.received != 0 {
                    last_receive_us = now_us;
                    receive_timeout_armed = true;
                }

                // Only declare idle after reception actually found no
                // pending transport data. Budget exhaustion alone is
                // not evidence of an idle UART.
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
                receive_errors += 1;
                diagnostic_pending = true;
                last_receive_us = board.timer.get_counter_low();

                processor.on_receive_timeout();
                receive_timeout_armed = false;

                let message = match error {
                    uart_transport::UartTransportError::Overrun => {
                        "Protocol UART RX overrun"
                    }
                    uart_transport::UartTransportError::Break => {
                        "Protocol UART RX break"
                    }
                    uart_transport::UartTransportError::Parity => {
                        "Protocol UART RX parity error"
                    }
                    uart_transport::UartTransportError::Framing => {
                        "Protocol UART RX framing error"
                    }
                };

                let _ = logger.log(LogLevel::Error, message);
            }
            Err(error) => {
                panic!(
                    "Protocol processing failed: {:?}",
                    error
                );
            }
        }

        logger
            .service(16)
            .expect("Debug UART output is infallible");

        let now_us = board.timer.get_counter_low();

        // Cumulative progress from successful poll calls only.
        // A failed poll may have consumed bytes before reporting its error.
        // Emit once after a receive pause, avoiding continuous benchmark logging.
        if diagnostic_pending && now_us.wrapping_sub(last_receive_us) >= 500_000
        {
            if log_message(
                &mut logger,
                format_args!(
                    "rx={} req={} tx={} err={} drop={}",
                    received_bytes,
                    processed_requests,
                    transmitted_bytes,
                    receive_errors,
                    discarded_events,
                ),
            )
            .is_ok()
            {
                diagnostic_pending = false;
            }
        }

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
