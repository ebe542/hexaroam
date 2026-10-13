//! PIO-based servo signal generation.

use rp_pico::hal::{
    gpio::{FunctionPio0, Pin, PullNone, bank0::Gpio0},
    pac::PIO0,
    pio::{
        InstalledProgram, PIO, PIO0SM0, PIOBuilder, PinDir, PinState, Running,
        StateMachine, Tx, UninitStateMachine,
    },
};

const PIO_CLOCK_HZ: u32 = 1_000_000;
const PERIOD_US: u32 = 20_000;
const PULSE_US: u32 = 1_500;

pub struct ServoPio {
    _state_machine: StateMachine<PIO0SM0, Running>,
    _tx: Tx<PIO0SM0>,
    _pin: Pin<Gpio0, FunctionPio0, PullNone>,
}

impl ServoPio {
    pub fn new(
        pio: &mut PIO<PIO0>,
        sm: UninitStateMachine<PIO0SM0>,
        pin: Pin<Gpio0, FunctionPio0, PullNone>,
        system_clock_hz: u32,
    ) -> Self {
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

        let (mut state_machine, _rx, mut tx) =
            PIOBuilder::from_installed_program(program)
                .set_pins(0, 1)
                .clock_divisor_fixed_point(divider as u16, 0)
                .build(sm);

        state_machine.set_pins([(0, PinState::Low)]);
        state_machine.set_pindirs([(0, PinDir::Output)]);

        assert!(
            tx.write(PULSE_US - 3),
            "Failed to queue high-phase count",
        );

        assert!(
            tx.write(PERIOD_US - PULSE_US - 3),
            "Failed to queue low-phase count",
        );

        Self {
            _state_machine: state_machine.start(),
            _tx: tx,
            _pin: pin,
        }
    }

    fn install_program(pio: &mut PIO<PIO0>) -> InstalledProgram<PIO0> {
        let program = pio_proc::pio_asm!(
            "pull block",
            "mov isr, osr",
            "pull block",
            ".wrap_target",
            "set pins, 1",
            "mov x, isr",
            "high:",
            "jmp x-- high",
            "set pins, 0",
            "mov x, osr",
            "low:",
            "jmp x-- low",
            ".wrap",
        );

        pio.install(&program.program)
            .expect("Insufficient PIO instruction memory")
    }
}
