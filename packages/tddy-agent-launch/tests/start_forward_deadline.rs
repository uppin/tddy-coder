//! A `StartSession` forwarded to a peer daemon waits long enough for a person to answer a
//! passphrase prompt.
//!
//! What is pinned is the deadline the forward is given, not that the forward honours it over
//! LiveKit: that needs the LiveKit testkit's docker container and belongs to the multi-host
//! acceptance suites. The value is the part that was wrong — the plain forward used the 30 s
//! peer-forward deadline while a split start, which has the same peer-side work, already waited the
//! peer's whole clone budget.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-17-clone-passphrase-prompt.md (AC11)

use std::time::Duration;

use tddy_agent_launch::start_forward_deadline;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::host_prompts::PROMPT_TTL;
use tddy_session_agents::agent_roster::split_forward_deadline;

#[test]
fn a_forwarded_start_waits_longer_than_a_passphrase_prompt_lives() {
    // Given a daemon on its default configuration
    let config = DaemonConfig::default();

    // When it decides how long to wait for a peer's answer to a forwarded start
    let deadline = start_forward_deadline(&config);

    // Then an operator who takes the whole prompt lifetime to answer is still waited for — and so
    // is the clone that follows. The 30 s forward deadline fails this by a factor of four.
    assert!(
        deadline > PROMPT_TTL,
        "{deadline:?} cannot outlast a {PROMPT_TTL:?} prompt"
    );
    assert_ne!(
        deadline,
        Duration::from_secs(30),
        "that is the ordinary forward deadline"
    );
}

#[test]
fn a_forwarded_start_gets_the_budget_a_split_start_already_has() {
    // Given a daemon on its default configuration
    let config = DaemonConfig::default();

    // Then the two forwards of the same peer-side work wait the same time: one budget to tune and
    // to reason about, not two that drift apart
    assert_eq!(
        start_forward_deadline(&config),
        split_forward_deadline(&config)
    );
}
