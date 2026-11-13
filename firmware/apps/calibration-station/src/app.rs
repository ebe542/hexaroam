//! Calibration-station hardware assembly and nonblocking application loop.

use crate::{
    uart_logging::UartLogOutput,
    uart_transport::{UartTransport, UartTransportError},
};
use calibration_station::{
    control::{CalibrationControl, OUTPUT_ERROR},
    frame_processor::{FrameProcessor, ProcessingError},
    pio_pulse_backend::{PERIOD_US, PioPulseBackend},
};
use embedded_hal::digital::OutputPin;
use hexaroam_logging::{LogLevel, Logger};
use hexaroam_servo_pio::{OutputBuffer, PreparedOutput, SwitchingOutput};
use hexaroam_servo_signal::PulseLimits;
use hexaroam_servo_storage::{Checksum, SoftwareCrc32};
use rp_pico::hal::{
    Timer,
    clocks::{Clock, PeripheralClock},
    dma::DMAExt,
    gpio::{FunctionPio0, FunctionUart, PullNone},
    pac,
    pio::PIOExt,
    uart::{Pins, UartConfig, UartPeripheral},
};

// Initial bench settings; not a guaranteed transport deadline.
const RECEIVE_IDLE_TIMEOUT_US: u32 = 100_000;
const RECEIVE_BUDGET: usize = 32;
const TRANSMIT_BUDGET: usize = 32;

pub struct Resources {
    pub pins: rp_pico::Pins,
    pub resets: pac::RESETS,
    pub pio: pac::PIO0,
    pub dma: pac::DMA,
    pub protocol_uart: pac::UART0,
    pub debug_uart: pac::UART1,
    pub peripheral_clock: PeripheralClock,
    pub system_clock_hz: u32,
    pub timer: Timer,
}

fn checksum(bytes: &[u8]) -> u32 {
    SoftwareCrc32.checksum(bytes)
}

pub fn run(mut resources: Resources) -> ! {
    let debug_pins = Pins::default().tx(resources
        .pins
        .gpio20
        .into_function::<FunctionUart>());

    let debug_uart = UartPeripheral::new(
        resources.debug_uart,
        debug_pins,
        &mut resources.resets,
    )
    .enable(
        UartConfig::default(),
        resources.peripheral_clock.freq(),
    )
    .expect("Failed to initialize debug UART");

    let mut logger = Logger::<_, 256>::new(UartLogOutput::new(debug_uart));

    let protocol_pins = Pins::default()
        .tx(resources
            .pins
            .gpio16
            .into_function::<FunctionUart>())
        .rx(resources
            .pins
            .gpio17
            .into_function::<FunctionUart>());

    let protocol_uart = UartPeripheral::new(
        resources.protocol_uart,
        protocol_pins,
        &mut resources.resets,
    )
    .enable(
        UartConfig::default(),
        resources.peripheral_clock.freq(),
    )
    .expect("Failed to initialize protocol UART");

    let mut transport = UartTransport::new(protocol_uart);

    let (mut pio, sm0, _, _, _) = resources
        .pio
        .split(&mut resources.resets);
    let dma = resources
        .dma
        .split(&mut resources.resets);

    let output_pins = [resources
        .pins
        .gpio0
        .into_function::<FunctionPio0>()
        .into_pull_type::<PullNone>()
        .into_dyn_pin()];

    let first_buffer = cortex_m::singleton!(
        : OutputBuffer<4> = OutputBuffer::new()
    )
    .expect("First output buffer already allocated");

    let second_buffer = cortex_m::singleton!(
        : OutputBuffer<4> = OutputBuffer::new()
    )
    .expect("Second output buffer already allocated");

    // PIO IRQ flag 0 belongs to output-underflow detection.
    pio.clear_irq(1);

    let prepared = PreparedOutput::new(
        &mut pio,
        sm0,
        output_pins,
        resources.system_clock_hz,
    )
    .expect("Failed to prepare servo output");

    let output = SwitchingOutput::new(
        prepared,
        dma.ch0,
        first_buffer,
        second_buffer,
        PERIOD_US,
    )
    .expect("Failed to initialize inactive servo output");

    let limits =
        PulseLimits::new(1_000, 2_000).expect("Valid initial bench limits");

    let backend = PioPulseBackend::new(output, limits)
        .expect("Failed to initialize pulse backend");

    let control = CalibrationControl::new(backend, limits);
    let mut processor = FrameProcessor::new(control);

    assert_eq!(processor.status(), 0);

    let mut led = resources
        .pins
        .led
        .into_push_pull_output();
    led.set_low()
        .expect("Failed to initialize LED");

    logger
        .log(
            LogLevel::Info,
            "Calibration station ready",
        )
        .expect("Startup message must fit");

    let mut led_high = false;
    let mut last_toggle_us = resources.timer.get_counter_low();
    let mut last_receive_us = last_toggle_us;
    let mut receive_timeout_armed = false;
    let mut output_error_reported = false;

    loop {
        // poll() always services the output before protocol work.
        match processor.poll(
            &mut transport,
            RECEIVE_BUDGET,
            TRANSMIT_BUDGET,
            &mut checksum,
        ) {
            Ok(progress) => {
                let now_us = resources.timer.get_counter_low();

                if progress.received != 0 {
                    last_receive_us = now_us;
                    receive_timeout_armed = true;
                }

                // Exhausting the receive budget does not prove UART idle.
                // Apply timeout only after a read found no available byte.
                if progress.receive_empty
                    && receive_timeout_armed
                    && now_us.wrapping_sub(last_receive_us)
                        >= RECEIVE_IDLE_TIMEOUT_US
                {
                    processor.on_receive_timeout();
                    receive_timeout_armed = false;
                }
            }

            Err(ProcessingError::Receive(error)) => {
                // The HAL consumed an erroneous UART entry.
                // Discard partial framing and wait for synchronization.
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

            Err(ProcessingError::Transmit(_)) => {
                // Pending response bytes remain owned by the processor.
                // The current UART driver cannot normally produce this error.
                let _ = logger.log(
                    LogLevel::Error,
                    "Protocol UART TX error",
                );
            }

            Err(ProcessingError::Submission(error)) => {
                // Responses fit the fixed frame capacity, and reception
                // is blocked while another response remains pending.
                panic!(
                    "Response submission failed: {:?}",
                    error
                );
            }
        }

        if processor.status() & OUTPUT_ERROR != 0 && !output_error_reported {
            output_error_reported = true;
            let _ = logger.log(LogLevel::Error, "Servo output fault");
        }

        logger
            .service(16)
            .expect("Debug UART output is infallible");

        // Heartbeat indicates loop activity, independently of output state.
        let now_us = resources.timer.get_counter_low();

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
