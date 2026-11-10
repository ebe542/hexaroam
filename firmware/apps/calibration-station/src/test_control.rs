use super::*;

#[derive(Default)]
struct FakeBackend {
    active: bool,
    faulted: bool,
    error: Option<BackendError>,
    starts: usize,
    updates: usize,
    stops: usize,
    services: usize,
    pulse: Option<u32>,
}

impl PulseBackend for FakeBackend {
    fn state(&self) -> SignalOutputState {
        if self.active {
            SignalOutputState::Active
        } else {
            SignalOutputState::Inactive
        }
    }

    fn is_faulted(&self) -> bool {
        self.faulted
    }

    fn start(&mut self, pulse_width_us: u32) -> Result<(), BackendError> {
        self.starts += 1;
        if let Some(error) = self.error {
            return Err(error);
        }
        self.active = true;
        self.pulse = Some(pulse_width_us);
        Ok(())
    }

    fn set_pulse(&mut self, pulse_width_us: u32) -> Result<(), BackendError> {
        self.updates += 1;
        if let Some(error) = self.error {
            return Err(error);
        }
        self.pulse = Some(pulse_width_us);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), BackendError> {
        self.stops += 1;
        if let Some(error) = self.error {
            return Err(error);
        }
        self.active = false;
        Ok(())
    }

    fn service(&mut self) {
        self.services += 1;
    }
}

fn control() -> CalibrationControl<FakeBackend> {
    CalibrationControl::new(
        FakeBackend::default(),
        PulseLimits::new(1_000, 2_000).unwrap(),
    )
}

fn running() -> CalibrationControl<FakeBackend> {
    let mut control = control();
    control.set_pulse(1_500).unwrap();
    control.set_enabled(true).unwrap();
    control
}

#[test]
fn startup_has_no_pulse_and_does_not_touch_output() {
    let control = control();

    assert_eq!(control.status(), 0);
    assert_eq!(control.pulse_width_us(), None);
    assert_eq!(control.backend.starts, 0);
    assert_eq!(control.backend.updates, 0);
    assert_eq!(control.backend.stops, 0);
}

#[test]
fn inactive_pulse_is_prepared_without_hardware_access() {
    let mut control = control();

    for pulse in [1_000, 1_500, 2_000] {
        assert_eq!(control.set_pulse(pulse), Ok(()));
        assert_eq!(control.pulse_width_us(), Some(pulse));
        assert_eq!(control.status(), PULSE_READY);
    }

    assert_eq!(control.backend.starts, 0);
    assert_eq!(control.backend.updates, 0);
}

#[test]
fn invalid_pulse_preserves_prepared_value() {
    let mut control = control();
    control.set_pulse(1_500).unwrap();

    for pulse in [0, 999, 2_001, u32::MAX] {
        assert_eq!(
            control.set_pulse(pulse),
            Err(ControlError::OutOfRange)
        );
        assert_eq!(control.pulse_width_us(), Some(1_500));
        assert_eq!(control.status(), PULSE_READY);
    }

    assert_eq!(control.backend.updates, 0);
}

#[test]
fn enable_requires_a_prepared_pulse() {
    let mut control = control();

    assert_eq!(
        control.set_enabled(true),
        Err(ControlError::MissingPulse)
    );
    assert_eq!(control.status(), 0);
    assert_eq!(control.backend.starts, 0);
}

#[test]
fn enable_starts_the_prepared_pulse_once() {
    let mut control = control();
    control.set_pulse(1_500).unwrap();

    assert_eq!(control.set_enabled(true), Ok(()));
    assert_eq!(control.set_enabled(true), Ok(()));
    assert_eq!(control.backend.starts, 1);
    assert_eq!(control.backend.pulse, Some(1_500));
    assert_eq!(control.status(), PULSE_READY | RUNNING);
}

#[test]
fn active_update_reaches_backend() {
    let mut control = running();

    assert_eq!(control.set_pulse(1_700), Ok(()));
    assert_eq!(control.backend.updates, 1);
    assert_eq!(control.backend.pulse, Some(1_700));
    assert_eq!(control.pulse_width_us(), Some(1_700));
    assert_eq!(control.status(), PULSE_READY | RUNNING);
}

#[test]
fn invalid_active_update_does_not_reach_backend() {
    let mut control = running();

    assert_eq!(
        control.set_pulse(2_001),
        Err(ControlError::OutOfRange)
    );
    assert_eq!(control.backend.updates, 0);
    assert_eq!(control.backend.pulse, Some(1_500));
    assert_eq!(control.pulse_width_us(), Some(1_500));
}

#[test]
fn busy_update_preserves_previous_value_without_fault() {
    let mut control = running();
    control.backend.error = Some(BackendError::Busy);

    assert_eq!(
        control.set_pulse(1_700),
        Err(ControlError::Busy)
    );
    assert_eq!(control.pulse_width_us(), Some(1_500));
    assert_eq!(control.backend.pulse, Some(1_500));
    assert_eq!(control.status(), PULSE_READY | RUNNING);
}

#[test]
fn disable_preserves_pulse_and_allows_restart() {
    let mut control = running();

    assert_eq!(control.set_enabled(false), Ok(()));
    assert_eq!(control.set_enabled(false), Ok(()));
    assert_eq!(control.status(), PULSE_READY);
    assert_eq!(control.pulse_width_us(), Some(1_500));

    assert_eq!(control.set_enabled(true), Ok(()));
    assert_eq!(control.backend.starts, 2);
    assert_eq!(control.backend.pulse, Some(1_500));
}

#[test]
fn failed_start_latches_output_error() {
    let mut control = control();
    control.set_pulse(1_500).unwrap();
    control.backend.error = Some(BackendError::OutputError);

    assert_eq!(
        control.set_enabled(true),
        Err(ControlError::OutputError)
    );
    assert_eq!(
        control.status(),
        PULSE_READY | OUTPUT_ERROR
    );

    control.backend.error = None;
    assert_eq!(
        control.set_enabled(true),
        Err(ControlError::OutputError)
    );
    assert_eq!(control.backend.starts, 1);
}

#[test]
fn failed_update_latches_error_and_blocks_further_updates() {
    let mut control = running();
    control.backend.error = Some(BackendError::OutputError);

    assert_eq!(
        control.set_pulse(1_700),
        Err(ControlError::OutputError)
    );
    assert_eq!(control.pulse_width_us(), Some(1_500));

    control.backend.error = None;
    assert_eq!(
        control.set_pulse(1_800),
        Err(ControlError::OutputError)
    );
    assert_eq!(control.backend.updates, 1);
    assert_eq!(
        control.status(),
        PULSE_READY | RUNNING | OUTPUT_ERROR
    );
}

#[test]
fn failed_stop_does_not_report_inactive_output() {
    let mut control = running();
    control.backend.error = Some(BackendError::OutputError);

    assert_eq!(
        control.set_enabled(false),
        Err(ControlError::OutputError)
    );
    assert_eq!(
        control.status(),
        PULSE_READY | RUNNING | OUTPUT_ERROR
    );

    control.backend.error = None;
    assert_eq!(control.set_enabled(false), Ok(()));
    assert_eq!(
        control.status(),
        PULSE_READY | OUTPUT_ERROR
    );
}

#[test]
fn service_forwards_to_backend_and_latches_fault() {
    let mut control = running();
    control.backend.faulted = true;
    control.backend.active = false;

    control.service();

    assert_eq!(control.backend.services, 1);
    assert_eq!(
        control.status(),
        PULSE_READY | OUTPUT_ERROR
    );

    control.backend.faulted = false;
    control.service();

    assert_eq!(control.backend.services, 2);
    assert_eq!(
        control.status(),
        PULSE_READY | OUTPUT_ERROR
    );
    assert_eq!(
        control.set_enabled(true),
        Err(ControlError::OutputError)
    );
}

#[test]
fn healthy_service_does_not_start_output() {
    let mut control = control();

    control.service();

    assert_eq!(control.backend.services, 1);
    assert_eq!(control.backend.starts, 0);
    assert_eq!(control.status(), 0);
}
