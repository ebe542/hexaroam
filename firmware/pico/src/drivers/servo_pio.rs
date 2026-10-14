//! PIO-based servo signal generation.

use hexaroam_servo_signal::{PulseLimits, PulseTransport, submit_pulse};
use rp_pico::hal::{
    gpio::{FunctionPio0, Pin, PullNone, bank0::Gpio0},
    pac::PIO0,
    pio::{
        InstalledProgram, PIO, PIO0SM0, PIOBuilder, PinDir, PinState, Running,
        ShiftDirection, StateMachine, Tx, UninitStateMachine,
    },
};

pub use hexaroam_servo_signal::PulseError;

const PIO_CLOCK_HZ: u32 = 1_000_000;
const PERIOD_US: u32 = 20_000;
const MIN_SUPPORTED_PULSE_US: u32 = 2;
const MAX_SUPPORTED_PULSE_US: u32 = PERIOD_US - 6;

#[derive(Debug, PartialEq, Eq)]
pub enum ServoInitError {
    UnsupportedPulseLimits,
    InitialPulseOutOfRange,
}

struct PioTransport {
    tx: Tx<PIO0SM0>,
}

impl PulseTransport for PioTransport {
    fn is_empty(&self) -> bool {
        self.tx.is_empty()
    }

    fn write(&mut self, pulse_width_us: u32) -> bool {
        self.tx
            .write(ServoPio::encode_pulse(pulse_width_us))
    }
}

pub struct ServoPio {
    _state_machine: StateMachine<PIO0SM0, Running>,
    transport: PioTransport,
    limits: PulseLimits,
    _pin: Pin<Gpio0, FunctionPio0, PullNone>,
}

impl ServoPio {
    pub fn new(
        pio: &mut PIO<PIO0>,
        sm: UninitStateMachine<PIO0SM0>,
        pin: Pin<Gpio0, FunctionPio0, PullNone>,
        system_clock_hz: u32,
        limits: PulseLimits,
        initial_pulse_us: u32,
    ) -> Result<Self, ServoInitError> {
        if limits.min_us() < MIN_SUPPORTED_PULSE_US
            || limits.max_us() > MAX_SUPPORTED_PULSE_US
        {
            return Err(ServoInitError::UnsupportedPulseLimits);
        }

        if !limits.contains(initial_pulse_us) {
            return Err(ServoInitError::InitialPulseOutOfRange);
        }
        assert_eq!(
            system_clock_hz % PIO_CLOCK_HZ,
            0,
            "PIO clock requires an integer divider",
        );

        let divider = system_clock_hz / PIO_CLOCK_HZ;

        assert!(
            (1..=u16::MAX as u32).contains(&divider),
            "PIO clock divider is out of range",
        );

        let program = Self::install_program(pio);

        let (mut state_machine, _rx, tx) =
            PIOBuilder::from_installed_program(program)
                .set_pins(0, 1)
                .out_shift_direction(ShiftDirection::Right)
                .clock_divisor_fixed_point(divider as u16, 0)
                .build(sm);

        state_machine.set_pins([(0, PinState::Low)]);
        state_machine.set_pindirs([(0, PinDir::Output)]);

        let mut transport = PioTransport { tx };

        submit_pulse(
            &mut transport,
            &limits,
            initial_pulse_us,
        )
        .expect("Failed to queue initial pulse");

        Ok(Self {
            _state_machine: state_machine.start(),
            transport,
            limits,
            _pin: pin,
        })
    }

    fn encode_pulse(pulse_width_us: u32) -> u32 {
        // At 1 MHz, each PIO instruction cycle takes 1 us.
        // High: SET (1) + JMP loop (count + 1).
        let high_count = pulse_width_us - 2;
        // Low: SET (1) + OUT (1) + JMP loop (count + 1)
        //      + PULL (1) + MOV (1) + OUT (1).
        // The final three instructions run after wrapping,
        // before the next rising edge.
        let low_count = PERIOD_US - pulse_width_us - 6;

        (low_count << 16) | high_count
    }

    pub fn set_pulse_width_us(
        &mut self,
        pulse_width_us: u32,
    ) -> Result<(), PulseError> {
        submit_pulse(
            &mut self.transport,
            &self.limits,
            pulse_width_us,
        )
    }

    fn install_program(pio: &mut PIO<PIO0>) -> InstalledProgram<PIO0> {
        let program = pio_proc::pio_asm!(
            ".wrap_target",
            "pull noblock",
            "mov x, osr",
            "out y, 16",
            "set pins, 1",
            "high:",
            "jmp y-- high",
            "set pins, 0",
            "out y, 16",
            "low:",
            "jmp y-- low",
            ".wrap",
        );

        pio.install(&program.program)
            .expect("Insufficient PIO instruction memory")
    }
}
