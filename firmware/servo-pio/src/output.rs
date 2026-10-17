//! RP2040 hardware resources for multi-channel signal output.

use rp2040_hal::{
    dma::{Channel, ChannelIndex, single_buffer},
    gpio::{DynPinId, Pin, PullNone},
    pio::{
        PIO, PIOBuilder, PIOExt, PinDir, PinState, Running, ShiftDirection,
        StateMachine, StateMachineIndex, Stopped, Tx, UninitStateMachine,
    },
};

use hexaroam_servo_signal::SignalPlan;

use crate::{OutputBuffer, encoding::EncodingError, program};

const PIO_CLOCK_HZ: u32 = 25_000_000;

#[derive(Debug, PartialEq, Eq)]
pub enum OutputInitError {
    EmptyOutputs,
    InvalidGpio,
    DuplicateGpio,
    UnsupportedClock,
    InstructionMemory,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputStartError {
    InvalidPlan,
    InsufficientCapacity,
}

enum InitialTransfer {
    Complete,
    #[cfg(feature = "hardware-tests")]
    MissingFinalCounter,
}

pub struct PreparedOutput<P: PIOExt, SM: StateMachineIndex, const N: usize> {
    state_machine: StateMachine<(P, SM), Stopped>,
    tx: Tx<(P, SM)>,
    pins: [Pin<DynPinId, P::PinFunction, PullNone>; N],
    gpio_mask: u32,
}

impl<P: PIOExt, SM: StateMachineIndex, const N: usize>
    PreparedOutput<P, SM, N>
{
    pub fn new(
        pio: &mut PIO<P>,
        sm: UninitStateMachine<(P, SM)>,
        pins: [Pin<DynPinId, P::PinFunction, PullNone>; N],
        system_clock_hz: u32,
    ) -> Result<Self, OutputInitError> {
        if N == 0 {
            return Err(OutputInitError::EmptyOutputs);
        }

        let mut gpio_mask = 0_u32;

        for pin in &pins {
            let gpio = pin.id().num;

            if gpio >= 30 {
                return Err(OutputInitError::InvalidGpio);
            }

            let bit = 1_u32 << gpio;

            if gpio_mask & bit != 0 {
                return Err(OutputInitError::DuplicateGpio);
            }

            gpio_mask |= bit;
        }

        if system_clock_hz % PIO_CLOCK_HZ != 0 {
            return Err(OutputInitError::UnsupportedClock);
        }

        let divider = system_clock_hz / PIO_CLOCK_HZ;

        if !(1..=u16::MAX as u32).contains(&divider) {
            return Err(OutputInitError::UnsupportedClock);
        }

        let installed = pio
            .install(&program::program())
            .map_err(|_| OutputInitError::InstructionMemory)?;

        let (mut state_machine, _rx, tx) =
            PIOBuilder::from_installed_program(installed)
                .out_pins(0, 32)
                .out_shift_direction(ShiftDirection::Right)
                .autopull(false)
                .clock_divisor_fixed_point(divider as u16, 0)
                .build(sm);

        // Establish Low levels before enabling output directions.
        state_machine.set_pins(
            pins.iter()
                .map(|pin| (pin.id().num, PinState::Low)),
        );

        state_machine.set_pindirs(
            pins.iter()
                .map(|pin| (pin.id().num, PinDir::Output)),
        );

        Ok(Self {
            state_machine,
            tx,
            pins,
            gpio_mask,
        })
    }

    fn start_fixed_inner<
        CH: ChannelIndex,
        const WORD_CAPACITY: usize,
        const PLAN_CAPACITY: usize,
    >(
        mut self,
        channel: Channel<CH>,
        buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
        plan: &SignalPlan<PLAN_CAPACITY>,
        initial_transfer: InitialTransfer,
    ) -> Result<FixedOutput<P, SM, CH, N>, OutputStartError> {
        buffer
            .load(plan, self.gpio_mask)
            .map_err(|error| match error {
                EncodingError::InsufficientCapacity => {
                    OutputStartError::InsufficientCapacity
                }
                _ => OutputStartError::InvalidPlan,
            })?;

        // Freeze the payload for this fixed-plan experiment.
        // The static slice keeps DMA source memory valid permanently.
        let words: &'static [u32] = buffer.words();

        self.state_machine.clear_fifos();

        // Prefill one complete entry before PIO can request data.
        // A valid SignalPlan contains at least a High and a Low entry.
        for &word in &words[..2] {
            assert!(
                self.tx.write(word),
                "Empty FIFO must accept the initial entry",
            );
        }

        let initial_words = match initial_transfer {
            InitialTransfer::Complete => &words[2..],
            #[cfg(feature = "hardware-tests")]
            InitialTransfer::MissingFinalCounter => &words[2..words.len() - 1],
        };

        let transfer =
            single_buffer::Config::new(channel, initial_words, self.tx).start();

        let state_machine = self.state_machine.start();

        Ok(FixedOutput {
            _state_machine: state_machine,
            _pins: self.pins,
            words,
            transfer: Some(transfer),
        })
    }

    pub fn start_fixed<
        CH: ChannelIndex,
        const WORD_CAPACITY: usize,
        const PLAN_CAPACITY: usize,
    >(
        self,
        channel: Channel<CH>,
        buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<FixedOutput<P, SM, CH, N>, OutputStartError> {
        self.start_fixed_inner(
            channel,
            buffer,
            plan,
            InitialTransfer::Complete,
        )
    }

    /// Hardware test only: omit the final counter of the first transfer.
    #[cfg(feature = "hardware-tests")]
    pub fn start_with_missing_final_counter<
        CH: ChannelIndex,
        const WORD_CAPACITY: usize,
        const PLAN_CAPACITY: usize,
    >(
        self,
        channel: Channel<CH>,
        buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<FixedOutput<P, SM, CH, N>, OutputStartError> {
        self.start_fixed_inner(
            channel,
            buffer,
            plan,
            InitialTransfer::MissingFinalCounter,
        )
    }
}

type PlanTransfer<P, SM, CH> =
    single_buffer::Transfer<Channel<CH>, &'static [u32], Tx<(P, SM)>>;

pub struct FixedOutput<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const N: usize,
> {
    _state_machine: StateMachine<(P, SM), Running>,
    _pins: [Pin<DynPinId, P::PinFunction, PullNone>; N],
    words: &'static [u32],
    transfer: Option<PlanTransfer<P, SM, CH>>,
}

impl<P: PIOExt, SM: StateMachineIndex, CH: ChannelIndex, const N: usize>
    FixedOutput<P, SM, CH, N>
{
    pub fn service(&mut self) {
        let completed = self
            .transfer
            .as_ref()
            .expect("DMA transfer must be present")
            .is_done();

        if !completed {
            return;
        }

        let transfer = self
            .transfer
            .take()
            .expect("DMA transfer must be present");

        let (channel, _previous_source, tx) = transfer.wait();

        self.transfer =
            Some(single_buffer::Config::new(channel, self.words, tx).start());
    }
}
