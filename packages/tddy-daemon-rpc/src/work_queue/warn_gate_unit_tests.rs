//! [`WarnGate`]: one log line per new reason, not one per poll.

use super::warn_gate::WarnGate;

const NO_VAULT: &str = "no credential vault is available to this session start";

#[test]
fn warns_the_first_time_a_reason_appears() {
    let gate = WarnGate::new();
    assert!(gate.should_warn("feature/x", NO_VAULT));
}

#[test]
fn stays_quiet_while_the_same_reason_repeats() {
    let gate = WarnGate::new();
    gate.should_warn("feature/x", NO_VAULT);
    for _ in 0..1_000 {
        assert!(!gate.should_warn("feature/x", NO_VAULT));
    }
}

#[test]
fn warns_again_when_the_reason_changes() {
    let gate = WarnGate::new();
    gate.should_warn("feature/x", NO_VAULT);
    assert!(gate.should_warn("feature/x", "the session's GitHub user is unknown"));
}

#[test]
fn keys_are_independent() {
    let gate = WarnGate::new();
    gate.should_warn("feature/x", NO_VAULT);
    assert!(gate.should_warn("feature/y", NO_VAULT));
}

#[test]
fn warns_again_after_the_condition_cleared() {
    let gate = WarnGate::new();
    gate.should_warn("feature/x", NO_VAULT);
    gate.clear("feature/x");
    assert!(gate.should_warn("feature/x", NO_VAULT));
}

#[test]
fn is_shared_across_threads() {
    fn assert_sync<T: Send + Sync>() {}
    assert_sync::<WarnGate>();
}
