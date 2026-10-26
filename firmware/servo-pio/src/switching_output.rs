//! Complete signal-plan replacement using two exclusive buffers.

use hexaroam_servo_signal::SignalPlan;
use rp2040_hal::{
    dma::{Channel, ChannelIndex, single_buffer},
    gpio::{DynPinId, Pin, PullNone},
    pac,
    pio::{PIOExt, Running, StateMachine, StateMachineIndex, Tx},
};

use crate::{
    OutputBuffer,
    encoding::EncodingError,
    output::{OutputStartError, PreparedOutput},
    pending_signal_plan::{PendingSignalPlan, SubmissionError},
    signal_plan_dma_source::SignalPlanDmaSource,
};

type PlanTransfer<P, SM, CH, const WORD_CAPACITY: usize> =
    single_buffer::Transfer<
        Channel<CH>,
        SignalPlanDmaSource<WORD_CAPACITY>,
        Tx<(P, SM)>,
    >;

#[derive(Debug, PartialEq, Eq)]
pub enum OutputSubmissionError {
    Busy,
    PeriodMismatch,
    OutputMismatch,
    InvalidPlan,
    InsufficientCapacity,
    Faulted,
}
use crate::dma_abort::abort_transfer;

const DMA_ABORT_POLL_BUDGET: usize = 100_000;

#[derive(Debug, PartialEq, Eq)]
pub enum OutputRestartError {
    NotFaulted,
    PeriodMismatch,
    OutputMismatch,
    InvalidPlan,
    InsufficientCapacity,
    DmaAbortTimeout,
}
fn submission_error(error: SubmissionError) -> OutputSubmissionError {
    match error {
        SubmissionError::Busy => OutputSubmissionError::Busy,
        SubmissionError::PeriodMismatch => {
            OutputSubmissionError::PeriodMismatch
        }
        SubmissionError::OutputMismatch => {
            OutputSubmissionError::OutputMismatch
        }
        SubmissionError::Encoding(EncodingError::InsufficientCapacity) => {
            OutputSubmissionError::InsufficientCapacity
        }
        SubmissionError::Encoding(_) => OutputSubmissionError::InvalidPlan,
    }
}

fn pio_fault_flag<P: PIOExt>() -> bool {
    let registers = match P::id() {
        0 => pac::PIO0::ptr(),
        1 => pac::PIO1::ptr(),
        _ => unreachable!("Unsupported PIO block"),
    };

    // SAFETY: Both addresses refer to RP2040 PIO register blocks.
    // This only reads the IRQ flags, without clearing or modifying them.
    // Flag 0 is reserved for this output driver's underflow indication.
    unsafe { (*registers).irq().read().irq().bits() & 1 != 0 }
}

fn clear_pio_fault_flag<P: PIOExt>() {
    let registers = match P::id() {
        0 => pac::PIO0::ptr(),
        1 => pac::PIO1::ptr(),
        _ => unreachable!("Unsupported PIO block"),
    };

    // SAFETY: IRQ is write-one-to-clear. Flag 0 is reserved
    // for this output driver; all other flags remain unchanged.
    unsafe {
        (*registers)
            .irq()
            .write(|w| w.irq().bits(1));
    }
}

pub struct SwitchingOutput<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const N: usize,
    const WORD_CAPACITY: usize,
> {
    state_machine: StateMachine<(P, SM), Running>,
    _pins: [Pin<DynPinId, P::PinFunction, PullNone>; N],
    transfer: Option<PlanTransfer<P, SM, CH, WORD_CAPACITY>>,
    pending: Option<PendingSignalPlan<'static, WORD_CAPACITY>>,
    period_us: u32,
    gpio_mask: u32,
    faulted: bool,
}

impl<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const N: usize,
    const WORD_CAPACITY: usize,
> SwitchingOutput<P, SM, CH, N, WORD_CAPACITY>
{
    pub fn start<const PLAN_CAPACITY: usize>(
        mut prepared: PreparedOutput<P, SM, N>,
        channel: Channel<CH>,
        first_buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
        second_buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<Self, OutputStartError> {
        if pio_fault_flag::<P>() {
            return Err(OutputStartError::Faulted);
        }

        let period_us = plan.period_us();
        let gpio_mask = prepared.gpio_mask;

        // Use the same complete-plan validation for startup and updates.
        let mut initial =
            PendingSignalPlan::new(first_buffer, period_us, gpio_mask);

        initial.submit(plan).map_err(|error| {
            match submission_error(error) {
                OutputSubmissionError::InsufficientCapacity => {
                    OutputStartError::InsufficientCapacity
                }
                _ => OutputStartError::InvalidPlan,
            }
        })?;

        let (first_buffer, _) = initial.into_parts();

        prepared.state_machine.clear_fifos();

        // Prefill a complete initial entry before enabling PIO.
        for &word in &first_buffer.words()[..2] {
            assert!(
                prepared.tx.write(word),
                "Empty FIFO must accept the initial entry",
            );
        }

        let word_count = first_buffer.words().len();
        let source = SignalPlanDmaSource::new(first_buffer, 2..word_count);

        let transfer =
            single_buffer::Config::new(channel, source, prepared.tx).start();

        let state_machine = prepared.state_machine.start();

        Ok(Self {
            state_machine: state_machine,
            _pins: prepared.pins,
            transfer: Some(transfer),
            pending: Some(PendingSignalPlan::new(
                second_buffer,
                period_us,
                gpio_mask,
            )),
            period_us,
            gpio_mask,
            faulted: false,
        })
    }

    /// Latches a detected PIO underflow until an explicit restart.
    ///
    /// Clearing the hardware IRQ flag does not clear this driver state.
    pub fn is_faulted(&mut self) -> bool {
        if pio_fault_flag::<P>() {
            self.faulted = true;
        }

        self.faulted
    }

    /// Accepts a complete replacement plan for a later output period.
    ///
    /// Success does not confirm that the plan is already active at GPIOs.
    pub fn submit_plan<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<(), OutputSubmissionError> {
        if self.is_faulted() {
            return Err(OutputSubmissionError::Faulted);
        }
        self.pending
            .as_mut()
            .expect("Pending-plan slot must be present")
            .submit(plan)
            .map_err(submission_error)
    }

    pub fn service(&mut self) {
        if self.is_faulted() {
            return;
        }
        if !self
            .transfer
            .as_ref()
            .expect("DMA transfer must be present")
            .is_done()
        {
            return;
        }

        let transfer = self
            .transfer
            .take()
            .expect("DMA transfer must be present");

        let (channel, source, tx) = transfer.wait();

        // DMA has finished reading this buffer. Words already in
        // the FIFO no longer depend on its RAM contents.
        let completed_buffer = source.into_buffer();

        let ready = self
            .pending
            .as_ref()
            .expect("Pending-plan slot must be present")
            .is_ready();

        let next_buffer = if ready {
            let pending = self
                .pending
                .take()
                .expect("Pending-plan slot must be present");

            let (ready_buffer, was_ready) = pending.into_parts();
            debug_assert!(was_ready);

            // The old DMA buffer becomes the next writable buffer.
            self.pending = Some(PendingSignalPlan::new(
                completed_buffer,
                self.period_us,
                self.gpio_mask,
            ));

            ready_buffer
        } else {
            // No replacement: repeat the complete previous plan.
            completed_buffer
        };

        let word_count = next_buffer.words().len();
        let source = SignalPlanDmaSource::new(next_buffer, 0..word_count);

        self.transfer =
            Some(single_buffer::Config::new(channel, source, tx).start());
    }

    /// Explicitly restarts faulted output with a complete signal plan.
    ///
    /// Validation failure preserves the previous pending plan.
    /// DMA abort timeout preserves transfer ownership and fault state.
    /// Success means the restart was initiated, not verified at the pins.
    pub fn restart<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<(), OutputRestartError> {
        if !self.is_faulted() {
            return Err(OutputRestartError::NotFaulted);
        }

        self.pending
            .as_mut()
            .expect("Pending-plan slot must be present")
            .prepare_restart(plan)
            .map_err(|error| match error {
                SubmissionError::PeriodMismatch => {
                    OutputRestartError::PeriodMismatch
                }
                SubmissionError::OutputMismatch => {
                    OutputRestartError::OutputMismatch
                }
                SubmissionError::Encoding(
                    EncodingError::InsufficientCapacity,
                ) => OutputRestartError::InsufficientCapacity,
                _ => OutputRestartError::InvalidPlan,
            })?;

        // Keep the transfer inside self until the abort is confirmed.
        // A timeout must not release its source memory.
        abort_transfer(
            self.transfer
                .as_mut()
                .expect("DMA transfer must be present"),
            DMA_ABORT_POLL_BUDGET,
        )
        .map_err(|_| OutputRestartError::DmaAbortTimeout)?;

        let transfer = self
            .transfer
            .take()
            .expect("DMA transfer must be present");

        // The abort confirmed BUSY is clear.
        // wait() now recovers the resources and applies the HAL's fences.
        let (channel, source, mut tx) = transfer.wait();
        let previous_buffer = source.into_buffer();

        let pending = self
            .pending
            .take()
            .expect("Pending-plan slot must be present");

        let (restart_buffer, ready) = pending.into_parts();
        debug_assert!(ready);

        // PIO remains in its fault loop and keeps the outputs Low.
        // DMA has stopped, so no old words can arrive after this clear.
        self.state_machine.clear_fifos();

        for &word in &restart_buffer.words()[..2] {
            assert!(
                tx.write(word),
                "Empty FIFO must accept the initial entry",
            );
        }

        let word_count = restart_buffer.words().len();
        let source = SignalPlanDmaSource::new(restart_buffer, 2..word_count);

        self.transfer =
            Some(single_buffer::Config::new(channel, source, tx).start());

        // The previous DMA buffer becomes writable again.
        // Its old payload is not marked as a pending replacement.
        self.pending = Some(PendingSignalPlan::new(
            previous_buffer,
            self.period_us,
            self.gpio_mask,
        ));

        clear_pio_fault_flag::<P>();

        // HAL restart pauses execution, jumps to the installed program's
        // wrap target, resets internal execution state, and enables PIO.
        // The initial entry and DMA source are already prepared.
        self.state_machine.restart();

        self.faulted = false;

        Ok(())
    }

    /// Hardware test only: reports whether the owned DMA transfer is active.
    #[cfg(feature = "hardware-tests")]
    pub fn dma_is_active(&self) -> bool {
        !self
            .transfer
            .as_ref()
            .expect("DMA transfer must be present")
            .is_done()
    }

    /// Hardware test only: stops PIO, forces Low, and blocks DMA on TX.
    ///
    /// Requires a current plan containing more than four encoded words.
    /// Leaves the driver faulted, with a full FIFO and active DMA.
    #[cfg(feature = "hardware-tests")]
    pub fn force_fault_with_active_dma(&mut self) {
        use pio::{
            Instruction, InstructionOperands, MovDestination, MovOperation,
            MovSource, OutDestination,
        };

        assert!(
            !self.is_faulted(),
            "Test requires running output"
        );

        let register_pointer = match P::id() {
            0 => pac::PIO0::ptr(),
            1 => pac::PIO1::ptr(),
            _ => unreachable!("Unsupported PIO block"),
        };

        // SAFETY: this driver owns the selected state machine.
        // Only its enable bit is cleared through the atomic alias.
        let registers = unsafe { &*register_pointer };

        unsafe {
            core::ptr::write_volatile(
                registers
                    .ctrl()
                    .as_ptr()
                    .byte_add(0x3000),
                1_u32 << SM::id(),
            );
        }

        // The type remains Running, but hardware execution is disabled.
        // The production restart method will reset and enable it again.
        self.faulted = true;

        for operands in [
            InstructionOperands::MOV {
                destination: MovDestination::OSR,
                op: MovOperation::None,
                source: MovSource::NULL,
            },
            InstructionOperands::OUT {
                destination: OutDestination::PINS,
                bit_count: 32,
            },
            InstructionOperands::IRQ {
                clear: false,
                wait: false,
                index: 0,
                relative: false,
            },
        ] {
            self.state_machine
                .exec_instruction(Instruction {
                    operands,
                    delay: 0,
                    side_set: None,
                });
        }

        // Retire the previous transfer before changing the FIFO or source.
        abort_transfer(
            self.transfer
                .as_mut()
                .expect("DMA transfer must be present"),
            DMA_ABORT_POLL_BUDGET,
        )
        .expect("Failed to abort DMA during test setup");

        let transfer = self
            .transfer
            .take()
            .expect("DMA transfer must be present");

        let (channel, source, tx) = transfer.wait();
        let buffer = source.into_buffer();
        let word_count = buffer.words().len();

        assert!(
            word_count > 4,
            "Test plan must exceed the four-word TX FIFO",
        );

        self.state_machine.clear_fifos();

        let source = SignalPlanDmaSource::new(buffer, 0..word_count);

        self.transfer =
            Some(single_buffer::Config::new(channel, source, tx).start());

        // PIO cannot consume words. A full FIFO must therefore leave
        // the longer DMA transfer blocked and still active.
        for _ in 0..DMA_ABORT_POLL_BUDGET {
            let fifo_full = registers.fstat().read().txfull().bits()
                & (1_u8 << SM::id())
                != 0;

            if fifo_full && self.dma_is_active() {
                return;
            }
        }

        panic!("Expected full FIFO and active DMA were not observed");
    }
}
