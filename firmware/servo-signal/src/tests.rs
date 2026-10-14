use super::*;

#[test]
fn limits_include_both_boundaries() {
    let limits = PulseLimits::new(1_000, 2_000).expect("Valid limits");

    assert!(!limits.contains(999));
    assert!(limits.contains(1_000));
    assert!(limits.contains(1_500));
    assert!(limits.contains(2_000));
    assert!(!limits.contains(2_001));
}

#[test]
fn reversed_limits_are_rejected() {
    assert_eq!(
        PulseLimits::new(2_000, 1_000),
        Err(LimitsError::MinExceedsMax),
    );
}

#[test]
fn equal_limits_allow_only_one_pulse_width() {
    let limits = PulseLimits::new(1_500, 1_500).expect("Valid limits");

    assert!(!limits.contains(1_499));
    assert!(limits.contains(1_500));
    assert!(!limits.contains(1_501));
}

#[derive(Default)]
struct TestTransport {
    pending: Option<u32>,
    write_calls: usize,
}

impl PulseTransport for TestTransport {
    fn is_empty(&self) -> bool {
        self.pending.is_none()
    }

    fn write(&mut self, pulse_width_us: u32) -> bool {
        self.write_calls += 1;

        if self.pending.is_some() {
            return false;
        }

        self.pending = Some(pulse_width_us);
        true
    }
}

#[test]
fn invalid_pulse_does_not_write_to_transport() {
    let limits = PulseLimits::new(1_000, 2_000).expect("Valid limits");

    for pulse_width_us in [0, 999, 2_001, u32::MAX] {
        let mut transport = TestTransport::default();

        assert_eq!(
            submit_pulse(&mut transport, &limits, pulse_width_us),
            Err(PulseError::OutOfRange),
        );

        assert_eq!(transport.write_calls, 0);
        assert_eq!(transport.pending, None);
    }
}

#[test]
fn occupied_transport_preserves_pending_pulse() {
    let limits = PulseLimits::new(1_000, 2_000).expect("Valid limits");

    let mut transport = TestTransport {
        pending: Some(1_500),
        write_calls: 0,
    };

    assert_eq!(
        submit_pulse(&mut transport, &limits, 2_000),
        Err(PulseError::Busy),
    );

    assert_eq!(transport.write_calls, 0);
    assert_eq!(transport.pending, Some(1_500));
}

#[test]
fn new_pulse_is_accepted_after_pending_pulse_is_consumed() {
    let limits = PulseLimits::new(1_000, 2_000).expect("Valid limits");

    let mut transport = TestTransport::default();

    assert_eq!(
        submit_pulse(&mut transport, &limits, 1_000),
        Ok(()),
    );
    assert_eq!(transport.pending, Some(1_000));
    assert_eq!(transport.write_calls, 1);

    assert_eq!(
        submit_pulse(&mut transport, &limits, 2_000),
        Err(PulseError::Busy),
    );
    assert_eq!(transport.pending, Some(1_000));
    assert_eq!(transport.write_calls, 1);

    // Simulate PIO consuming the pending pulse.
    assert_eq!(transport.pending.take(), Some(1_000));

    assert_eq!(
        submit_pulse(&mut transport, &limits, 2_000),
        Ok(()),
    );
    assert_eq!(transport.pending, Some(2_000));
    assert_eq!(transport.write_calls, 2);
}
