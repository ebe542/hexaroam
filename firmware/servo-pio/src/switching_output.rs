//! Complete signal-plan replacement using two exclusive buffers.

use hexaroam_servo_signal::{SignalOutputState, SignalPlan};
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

/// Owns either a running transfer or all resources recovered after stop.
enum DmaResources<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const WORD_CAPACITY: usize,
> {
    Running(PlanTransfer<P, SM, CH, WORD_CAPACITY>),
    Stopped {
        channel: Channel<CH>,
        buffer: &'static mut OutputBuffer<WORD_CAPACITY>,
        tx: Tx<(P, SM)>,
    },

    // Used only while moving owned resources inside a mutable operation.
    // A successful public operation must not leave this variant behind.
    Transition,
}

impl<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const WORD_CAPACITY: usize,
> DmaResources<P, SM, CH, WORD_CAPACITY>
{
    fn as_ref(&self) -> Option<&PlanTransfer<P, SM, CH, WORD_CAPACITY>> {
        match self {
            Self::Running(transfer) => Some(transfer),
            _ => None,
        }
    }

    fn as_mut(
        &mut self,
    ) -> Option<&mut PlanTransfer<P, SM, CH, WORD_CAPACITY>> {
        match self {
            Self::Running(transfer) => Some(transfer),
            _ => None,
        }
    }

    fn take(&mut self) -> Option<PlanTransfer<P, SM, CH, WORD_CAPACITY>> {
        match core::mem::replace(self, Self::Transition) {
            Self::Running(transfer) => Some(transfer),
            other => {
                *self = other;
                None
            }
        }
    }

    #[cfg(feature = "hardware-tests")]
    fn is_some(&self) -> bool {
        matches!(self, Self::Running(_))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputSubmissionError {
    Busy,
    PeriodMismatch,
    OutputMismatch,
    InvalidPlan,
    InsufficientCapacity,
    Faulted,
    Stopped,
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

#[derive(Debug, PartialEq, Eq)]
pub enum OutputStopError {
    DmaAbortTimeout,
}

/// Errors from regular lifecycle operations, distinct from fault recovery.
#[derive(Debug, PartialEq, Eq)]
pub enum OutputLifecycleError {
    AlreadyActive,
    Faulted,
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
    transfer: DmaResources<P, SM, CH, WORD_CAPACITY>,
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
            transfer: DmaResources::Running(transfer),
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

    /// Starts stopped, non-faulted output with a complete initial plan.
    ///
    /// Invalid plans preserve stopped resources and Low output levels.
    /// Starting active output is rejected without changing its plan.
    /// Ordinary startup never clears a latched fault.
    ///
    /// The borrowed plan is encoded before returning.
    /// Success confirms startup, not measured pulse timing.
    pub fn start_plan<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<(), OutputLifecycleError> {
        if self.is_faulted() {
            return Err(OutputLifecycleError::Faulted);
        }

        if self.state() != SignalOutputState::Inactive {
            return Err(OutputLifecycleError::AlreadyActive);
        }

        // Reuse transactional plan preparation without clearing any fault.
        // Validation/encoding failure preserves the buffer and ready state.
        self.pending
            .as_mut()
            .expect("Pending-plan slot must be present")
            .prepare_restart(plan)
            .map_err(|error| match error {
                SubmissionError::PeriodMismatch => {
                    OutputLifecycleError::PeriodMismatch
                }
                SubmissionError::OutputMismatch => {
                    OutputLifecycleError::OutputMismatch
                }
                SubmissionError::Encoding(
                    EncodingError::InsufficientCapacity,
                ) => OutputLifecycleError::InsufficientCapacity,
                SubmissionError::Encoding(_) => {
                    OutputLifecycleError::InvalidPlan
                }
                SubmissionError::Busy => {
                    unreachable!("Explicit preparation replaces pending data")
                }
            })?;

        // Stop already confirmed DMA shutdown. No abort is needed here.
        // All recoverable errors were handled before moving owned resources.
        let resources = core::mem::replace(
            &mut self.transfer,
            DmaResources::Transition,
        );

        let (channel, previous_buffer, mut tx) = match resources {
            DmaResources::Stopped {
                channel,
                buffer,
                tx,
            } => (channel, buffer, tx),
            _ => unreachable!("Startup requires stopped resources"),
        };

        let pending = self
            .pending
            .take()
            .expect("Prepared startup plan must be present");

        let (start_buffer, ready) = pending.into_parts();
        debug_assert!(ready);

        // PIO remains disabled and DMA is stopped.
        // Remove stale FIFO contents before loading the initial entry.
        self.state_machine.clear_fifos();

        for &word in &start_buffer.words()[..2] {
            assert!(
                tx.write(word),
                "Empty FIFO must accept the initial entry",
            );
        }

        let word_count = start_buffer.words().len();
        let source = SignalPlanDmaSource::new(start_buffer, 2..word_count);

        self.transfer = DmaResources::Running(
            single_buffer::Config::new(channel, source, tx).start(),
        );

        // Old source contents must not become a pending replacement.
        self.pending = Some(PendingSignalPlan::new(
            previous_buffer,
            self.period_us,
            self.gpio_mask,
        ));

        // Reset to the wrap target and enable PIO only after FIFO,
        // DMA and buffer ownership are ready. Do not clear fault flags.
        self.state_machine.restart();

        Ok(())
    }

    /// Stops PIO output and confirms DMA shutdown.
    ///
    /// Success leaves assigned signal pins Low and discards any pending
    /// replacement plan. Repeated stop succeeds without restarting output.
    /// Existing faults remain latched.
    ///
    /// On DMA abort timeout, transfer and buffer ownership are retained.
    /// Inactive must not be reported until shutdown is confirmed.
    pub fn stop(&mut self) -> Result<(), OutputStopError> {
        if self.state() == SignalOutputState::Inactive {
            return Ok(());
        }

        // Capture an existing hardware fault before changing execution.
        self.is_faulted();

        // Prevent further edges before waiting for DMA shutdown.
        self.halt_and_force_low();

        let result = abort_transfer(
            self.transfer
                .as_mut()
                .expect("Running DMA resources must be present"),
            DMA_ABORT_POLL_BUDGET,
        );

        if result.is_err() {
            // DMA may still access its source. Keep the complete transfer
            // owned and prevent ordinary servicing from restarting it.
            self.faulted = true;
            return Err(OutputStopError::DmaAbortTimeout);
        }

        let transfer = self
            .transfer
            .take()
            .expect("Confirmed DMA transfer must be present");

        // Abort completion was confirmed before recovering resources.
        let (channel, source, tx) = transfer.wait();
        let buffer = source.into_buffer();

        // DMA can no longer refill the FIFO.
        self.state_machine.clear_fifos();

        // Keep the second buffer, but discard its pending replacement.
        let pending = self
            .pending
            .take()
            .expect("Pending-plan slot must be present");

        let (pending_buffer, _) = pending.into_parts();

        self.pending = Some(PendingSignalPlan::new(
            pending_buffer,
            self.period_us,
            self.gpio_mask,
        ));

        self.transfer = DmaResources::Stopped {
            channel,
            buffer,
            tx,
        };

        Ok(())
    }

    /// Reports whether output has been confirmed stopped.
    ///
    /// Active also covers an incomplete stop after a technical error.
    /// This does not report instantaneous pin levels or servo position.
    pub fn state(&self) -> SignalOutputState {
        match &self.transfer {
            DmaResources::Stopped { .. } => SignalOutputState::Inactive,
            _ => SignalOutputState::Active,
        }
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
        if self.state() == SignalOutputState::Inactive {
            return Err(OutputSubmissionError::Stopped);
        }
        self.pending
            .as_mut()
            .expect("Pending-plan slot must be present")
            .submit(plan)
            .map_err(submission_error)
    }

    pub fn service(&mut self) {
        if self.state() == SignalOutputState::Inactive {
            return;
        }
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

        self.transfer = DmaResources::Running(
            single_buffer::Config::new(channel, source, tx).start(),
        );
    }

    /// Explicitly restarts faulted output with a complete signal plan.
    ///
    /// Validation failure preserves the previous pending plan.
    /// DMA abort timeout preserves transfer ownership and fault state.
    /// Success means the restart was initiated, not verified at the pins.
    fn restart_inner<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
        abort_poll_budget: usize,
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

        // Fault recovery may own a running transfer or resources already
        // recovered by stop. Abort only a transfer that is still owned.
        if let Some(transfer) = self.transfer.as_mut() {
            abort_transfer(transfer, abort_poll_budget)
                .map_err(|_| OutputRestartError::DmaAbortTimeout)?;
        }

        // All recoverable errors precede this ownership transition.
        let resources = core::mem::replace(
            &mut self.transfer,
            DmaResources::Transition,
        );

        let (channel, previous_buffer, mut tx) = match resources {
            DmaResources::Running(transfer) => {
                let (channel, source, tx) = transfer.wait();
                (channel, source.into_buffer(), tx)
            }
            DmaResources::Stopped {
                channel,
                buffer,
                tx,
            } => (channel, buffer, tx),
            DmaResources::Transition => {
                unreachable!("DMA resources must be owned")
            }
        };

        let pending = self
            .pending
            .take()
            .expect("Pending-plan slot must be present");

        let (restart_buffer, ready) = pending.into_parts();
        debug_assert!(ready);

        // PIO is either in its fault loop or explicitly halted.
        // Assigned outputs remain Low until the explicit restart.
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

        self.transfer = DmaResources::Running(
            single_buffer::Config::new(channel, source, tx).start(),
        );

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
        self.transfer
            .as_ref()
            .is_some_and(|transfer| !transfer.is_done())
    }

    /// Hardware test only: stops PIO, forces Low, and blocks DMA on TX.
    ///
    /// Requires a current plan containing more than four encoded words.
    /// Leaves the driver faulted, with a full FIFO and active DMA.
    #[cfg(feature = "hardware-tests")]
    pub fn force_fault_with_active_dma(&mut self) {
        assert_eq!(
            self.state(),
            SignalOutputState::Active,
            "Test requires active output",
        );
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

        self.transfer = DmaResources::Running(
            single_buffer::Config::new(channel, source, tx).start(),
        );

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

    /// Explicitly restarts faulted output with a complete signal plan.
    pub fn restart<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<(), OutputRestartError> {
        self.restart_inner(plan, DMA_ABORT_POLL_BUDGET)
    }

    /// Hardware test only: requests DMA abort without polling completion.
    ///
    /// A valid restart request deterministically returns DmaAbortTimeout.
    /// DMA may nevertheless stop asynchronously after the abort request.
    #[cfg(feature = "hardware-tests")]
    pub fn restart_with_zero_abort_budget<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<(), OutputRestartError> {
        self.restart_inner(plan, 0)
    }

    /// Hardware test only: confirms the transfer remains owned.
    #[cfg(feature = "hardware-tests")]
    pub fn has_dma_transfer(&self) -> bool {
        self.transfer.is_some()
    }

    /// Disables this state machine and forces its output latch Low.
    ///
    /// DMA may still be running; this method does not release resources.
    fn halt_and_force_low(&mut self) {
        use pio::{
            Instruction, InstructionOperands, MovDestination, MovOperation,
            MovSource, OutDestination,
        };

        let register_pointer = match P::id() {
            0 => pac::PIO0::ptr(),
            1 => pac::PIO1::ptr(),
            _ => unreachable!("Unsupported PIO block"),
        };

        // SAFETY: This driver owns the selected state machine.
        // The atomic-clear alias changes only its enable bit.
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

        // As in the existing fault-injection path, the HAL handle retains
        // its Running type while hardware execution is disabled.
        // Only an explicit restart may enable execution again.
        //
        // OUT uses the driver's configured 32-bit output mapping.
        // Only the assigned pins are owned/configured as this PIO output.
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
        ] {
            self.state_machine
                .exec_instruction(Instruction {
                    operands,
                    delay: 0,
                    side_set: None,
                });
        }
    }
}

impl<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const N: usize,
    const WORD_CAPACITY: usize,
> hexaroam_servo_signal::SignalPlanOutput
    for SwitchingOutput<P, SM, CH, N, WORD_CAPACITY>
{
    type Error = OutputSubmissionError;

    fn submit_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::Error> {
        SwitchingOutput::submit_plan(self, plan)
    }

    fn service(&mut self) {
        SwitchingOutput::service(self);
    }
}

impl<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const N: usize,
    const WORD_CAPACITY: usize,
> hexaroam_servo_signal::SignalPlanLifecycle
    for SwitchingOutput<P, SM, CH, N, WORD_CAPACITY>
{
    type LifecycleError = OutputLifecycleError;

    fn start_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::LifecycleError> {
        SwitchingOutput::start_plan(self, plan)
    }

    fn stop(&mut self) -> Result<(), Self::LifecycleError> {
        SwitchingOutput::stop(self).map_err(|error| match error {
            OutputStopError::DmaAbortTimeout => {
                OutputLifecycleError::DmaAbortTimeout
            }
        })
    }

    fn state(&self) -> SignalOutputState {
        SwitchingOutput::state(self)
    }
}
