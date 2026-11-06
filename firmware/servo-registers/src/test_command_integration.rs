extern crate std;

use self::std::{cell::RefCell, rc::Rc, vec::Vec};
use super::*;
use crate::OutputBackend;
use hexaroam_servo_control::{
    Angle, Calibration, ServoBank, ServoId, ServoSignalOutput,
};
use hexaroam_servo_protocol::command::{Request, decode};
use hexaroam_servo_signal::{
    PulseLimits, SignalOutputState, SignalPlan, SignalPlanEntry,
    SignalPlanLifecycle, SignalPlanOutput,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestError {
    Busy,
    Fault,
}

#[derive(Default)]
struct Recorded {
    active: bool,
    plan: Option<Vec<SignalPlanEntry>>,
    start_calls: usize,
    submit_calls: usize,
    stop_calls: usize,
    submission_error: Option<TestError>,
    stop_error: bool,
    service_calls: usize,
}

struct TestOutput {
    recorded: Rc<RefCell<Recorded>>,
}

impl SignalPlanOutput for TestOutput {
    type Error = TestError;

    fn is_busy_error(error: &Self::Error) -> bool {
        matches!(error, TestError::Busy)
    }

    fn submit_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::Error> {
        let mut recorded = self.recorded.borrow_mut();
        recorded.submit_calls += 1;

        if let Some(error) = recorded.submission_error {
            return Err(error);
        }

        assert!(recorded.active);
        assert_eq!(plan.period_us(), 20_000);
        recorded.plan = Some(plan.entries().to_vec());

        Ok(())
    }

    fn service(&mut self) {
        self.recorded.borrow_mut().service_calls += 1;
    }
}

impl SignalPlanLifecycle for TestOutput {
    type LifecycleError = TestError;

    fn start_plan<const CAPACITY: usize>(
        &mut self,
        plan: &SignalPlan<CAPACITY>,
    ) -> Result<(), Self::LifecycleError> {
        let mut recorded = self.recorded.borrow_mut();

        assert!(!recorded.active);
        assert_eq!(plan.period_us(), 20_000);

        recorded.start_calls += 1;
        recorded.plan = Some(plan.entries().to_vec());
        recorded.active = true;

        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::LifecycleError> {
        let mut recorded = self.recorded.borrow_mut();
        recorded.stop_calls += 1;

        if recorded.stop_error {
            return Err(TestError::Fault);
        }

        recorded.active = false;
        recorded.plan = None;

        Ok(())
    }

    fn state(&self) -> SignalOutputState {
        if self.recorded.borrow().active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        }
    }
}

type Signals = ServoSignalOutput<TestOutput, 2, 3>;
type Bank = ServoBank<Signals, 2>;
type Service = CommandService<Bank, 2>;

fn setup() -> (Service, Rc<RefCell<Recorded>>) {
    let recorded = Rc::new(RefCell::new(Recorded::default()));

    let signals = ServoSignalOutput::new(
        TestOutput {
            recorded: Rc::clone(&recorded),
        },
        [22, 26],
        PulseLimits::new(1_000, 2_000).unwrap(),
        20_000,
    )
    .unwrap();

    let calibration = |lower, upper| {
        Calibration::new(
            Angle::from_centidegrees(-4_500),
            lower,
            Angle::from_centidegrees(4_500),
            upper,
        )
        .unwrap()
    };

    let bank = ServoBank::new(
        [calibration(1_000, 2_000), calibration(1_200, 1_800)],
        signals,
    );

    (
        CommandService::new(bank, true),
        recorded,
    )
}

fn write(
    service: &mut Service,
    first: u8,
    angles: &[i32],
) -> Result<(), NackCode> {
    let mut content = [0; 10];
    content[0] = 0x03;
    content[1] = first;

    for (index, angle) in angles.iter().enumerate() {
        let offset = 2 + index * 4;
        content[offset..offset + 4].copy_from_slice(&angle.to_le_bytes());
    }

    let Some(Request::SetTargets(batch)) =
        decode(&content[..2 + angles.len() * 4]).unwrap()
    else {
        panic!("Expected target request");
    };

    service.set_targets(batch)
}

fn running() -> (Service, Rc<RefCell<Recorded>>) {
    let (mut service, recorded) = setup();
    service.set_enabled(true).unwrap();
    write(&mut service, 0, &[-4_500, 4_500]).unwrap();

    (service, recorded)
}

fn target(service: &Service, id: usize) -> i32 {
    service
        .output
        .target(ServoId(id))
        .unwrap()
        .as_centidegrees()
}

#[test]
fn enable_does_not_build_or_start_a_signal_plan() {
    let (mut service, recorded) = setup();

    service.set_enabled(true).unwrap();

    let recorded = recorded.borrow();
    assert_eq!(recorded.start_calls, 0);
    assert_eq!(recorded.submit_calls, 0);
    assert!(recorded.plan.is_none());
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED
    );
}

#[test]
fn initial_angles_generate_calibrated_plan_on_nonconsecutive_gpios() {
    let (service, recorded) = running();
    let recorded = recorded.borrow();

    assert_eq!(recorded.start_calls, 1);
    assert_eq!(recorded.submit_calls, 0);
    assert_eq!(
        recorded.plan.as_deref(),
        Some(
            &[
                SignalPlanEntry {
                    gpio_mask: (1 << 22) | (1 << 26),
                    duration_us: 1_000,
                },
                SignalPlanEntry {
                    gpio_mask: 1 << 26,
                    duration_us: 800,
                },
                SignalPlanEntry {
                    gpio_mask: 0,
                    duration_us: 18_200,
                },
            ][..]
        ),
    );
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
}

#[test]
fn partial_update_rebuilds_full_plan_and_preserves_omitted_angle() {
    let (mut service, recorded) = running();

    write(&mut service, 0, &[0]).unwrap();

    assert_eq!(target(&service, 0), 0);
    assert_eq!(target(&service, 1), 4_500);

    let recorded = recorded.borrow();
    assert_eq!(recorded.start_calls, 1);
    assert_eq!(recorded.submit_calls, 1);
    assert_eq!(
        recorded.plan.as_deref(),
        Some(
            &[
                SignalPlanEntry {
                    gpio_mask: (1 << 22) | (1 << 26),
                    duration_us: 1_500,
                },
                SignalPlanEntry {
                    gpio_mask: 1 << 26,
                    duration_us: 300,
                },
                SignalPlanEntry {
                    gpio_mask: 0,
                    duration_us: 18_200,
                },
            ][..]
        ),
    );
}

#[test]
fn calibration_failure_rejects_complete_batch_before_submission() {
    let (mut service, recorded) = running();
    let before = recorded.borrow().plan.clone();

    // The earlier fake backend allowed +/-90 degrees; real calibration
    // here is restricted to +/-45 degrees.
    assert_eq!(
        write(&mut service, 0, &[0, 4_501]),
        Err(NackCode::InvalidValue),
    );

    assert_eq!(target(&service, 0), -4_500);
    assert_eq!(target(&service, 1), 4_500);
    assert_eq!(recorded.borrow().submit_calls, 0);
    assert_eq!(recorded.borrow().plan, before);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
}

#[test]
fn busy_crosses_all_adapters_without_committing_targets_or_fault() {
    let (mut service, recorded) = running();
    let before = recorded.borrow().plan.clone();
    recorded.borrow_mut().submission_error = Some(TestError::Busy);

    assert_eq!(
        write(&mut service, 0, &[0, 0]),
        Err(NackCode::Busy),
    );

    assert_eq!(target(&service, 0), -4_500);
    assert_eq!(target(&service, 1), 4_500);
    assert_eq!(recorded.borrow().plan, before);
    assert_eq!(recorded.borrow().stop_calls, 0);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );

    recorded.borrow_mut().submission_error = None;
    assert_eq!(write(&mut service, 0, &[0, 0]), Ok(()));
    assert_eq!(target(&service, 0), 0);
    assert_eq!(target(&service, 1), 0);
}

#[test]
fn technical_plan_failure_latches_error_and_stops_real_bank() {
    let (mut service, recorded) = running();
    recorded.borrow_mut().submission_error = Some(TestError::Fault);

    assert_eq!(
        write(&mut service, 0, &[0, 0]),
        Err(NackCode::OutputError),
    );

    assert_eq!(target(&service, 0), -4_500);
    assert_eq!(target(&service, 1), 4_500);
    assert_eq!(recorded.borrow().stop_calls, 1);
    assert!(!recorded.borrow().active);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR
    );
    assert_eq!(
        service.set_enabled(true),
        Err(NackCode::OutputError),
    );
}

#[test]
fn failed_shutdown_keeps_running_visible_through_all_adapters() {
    let (mut service, recorded) = running();

    {
        let mut recorded = recorded.borrow_mut();
        recorded.submission_error = Some(TestError::Fault);
        recorded.stop_error = true;
    }

    assert_eq!(
        write(&mut service, 0, &[0, 0]),
        Err(NackCode::OutputError),
    );

    assert_eq!(
        service.status(),
        CALIBRATION_VALID | RUNNING | OUTPUT_ERROR,
    );

    recorded.borrow_mut().stop_error = false;
    service.set_enabled(false).unwrap();

    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR
    );
    assert_eq!(recorded.borrow().stop_calls, 2);
}

#[test]
fn restart_requires_full_new_pose_even_when_bank_retains_targets() {
    let (mut service, recorded) = running();

    service.set_enabled(false).unwrap();
    service.set_enabled(true).unwrap();

    assert_eq!(
        write(&mut service, 1, &[0]),
        Err(NackCode::InvalidState),
    );
    assert_eq!(recorded.borrow().start_calls, 1);
    assert!(!recorded.borrow().active);

    assert_eq!(write(&mut service, 0, &[0, 0]), Ok(()));
    assert_eq!(recorded.borrow().start_calls, 2);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
}

#[test]
fn servicing_disabled_output_does_not_start_or_change_status() {
    let (mut service, recorded) = setup();
    let before = service.status();

    service.service();

    let recorded = recorded.borrow();
    assert_eq!(recorded.service_calls, 1);
    assert_eq!(recorded.start_calls, 0);
    assert_eq!(recorded.submit_calls, 0);
    assert_eq!(recorded.stop_calls, 0);
    assert!(!recorded.active);
    assert_eq!(service.status(), before);
}

#[test]
fn servicing_running_output_does_not_submit_or_restart() {
    let (mut service, recorded) = running();
    let before = recorded.borrow().plan.clone();

    service.service();
    service.service();

    let recorded = recorded.borrow();
    assert_eq!(recorded.service_calls, 2);
    assert_eq!(recorded.start_calls, 1);
    assert_eq!(recorded.submit_calls, 0);
    assert_eq!(recorded.stop_calls, 0);
    assert_eq!(recorded.plan, before);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ENABLED | RUNNING,
    );
}

#[test]
fn servicing_after_fault_does_not_recover_or_enable_output() {
    let (mut service, recorded) = running();
    recorded.borrow_mut().submission_error = Some(TestError::Fault);

    assert_eq!(
        write(&mut service, 0, &[0, 0]),
        Err(NackCode::OutputError),
    );

    service.service();

    let recorded = recorded.borrow();
    assert_eq!(recorded.service_calls, 1);
    assert_eq!(recorded.start_calls, 1);
    assert_eq!(recorded.stop_calls, 1);
    assert!(!recorded.active);
    assert_eq!(
        service.status(),
        CALIBRATION_VALID | OUTPUT_ERROR
    );
}
