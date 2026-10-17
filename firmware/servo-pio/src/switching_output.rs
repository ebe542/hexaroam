//! Complete signal-plan replacement using two exclusive buffers.

use hexaroam_servo_signal::SignalPlan;
use rp2040_hal::{
    dma::{Channel, ChannelIndex, single_buffer},
    gpio::{DynPinId, Pin, PullNone},
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

pub struct SwitchingOutput<
    P: PIOExt,
    SM: StateMachineIndex,
    CH: ChannelIndex,
    const N: usize,
    const WORD_CAPACITY: usize,
> {
    _state_machine: StateMachine<(P, SM), Running>,
    _pins: [Pin<DynPinId, P::PinFunction, PullNone>; N],
    transfer: Option<PlanTransfer<P, SM, CH, WORD_CAPACITY>>,
    pending: Option<PendingSignalPlan<'static, WORD_CAPACITY>>,
    period_us: u32,
    gpio_mask: u32,
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
            _state_machine: state_machine,
            _pins: prepared.pins,
            transfer: Some(transfer),
            pending: Some(PendingSignalPlan::new(
                second_buffer,
                period_us,
                gpio_mask,
            )),
            period_us,
            gpio_mask,
        })
    }

    /// Accepts a complete replacement plan for a later output period.
    ///
    /// Success does not confirm that the plan is already active at GPIOs.
    pub fn submit_plan<const PLAN_CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<PLAN_CAPACITY>,
    ) -> Result<(), OutputSubmissionError> {
        self.pending
            .as_mut()
            .expect("Pending-plan slot must be present")
            .submit(plan)
            .map_err(submission_error)
    }

    pub fn service(&mut self) {
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
}
