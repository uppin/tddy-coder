//! Unit: how long `AwaitQuestionAnswer` holds one session's hook (`#agent-tools 19/19`).
//!
//! The hold equals the hook timeout the session was launched with: the wait its start recorded in
//! its session directory, else its agent spec's `coding_agent.answer_wait_secs`, else the specs'
//! default. No daemon-wide value is read.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-agent-config-removal.md`.

use std::path::Path;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{CodingAgentRegistry, DEFAULT_ANSWER_WAIT_SECS};
use tddy_session_activity::answer_wait::{
    record_answer_wait, recorded_answer_wait, session_answer_wait, ANSWER_WAIT_FILE,
};

/// The built-in claude.yaml overriding itself with `answer_wait_secs: <secs>` in its
/// `coding_agent` section.
fn claude_yaml_waiting(secs: u64) -> String {
    let (_, claude) = tddy_coding_agent_specs::builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with("/claude.yaml"))
        .expect("claude.yaml is built in");
    let section = "  coding_agent:\n";
    assert!(
        claude.contains(section),
        "claude.yaml has a coding_agent section"
    );
    claude.replacen(
        section,
        &format!("{section}    answer_wait_secs: {secs}\n"),
        1,
    )
}

/// The registry of a daemon whose `<tddyhome>/tools` holds `tools` (`(file name, YAML)`).
struct ARegistry {
    agents: CodingAgentRegistry,
    _tools: tempfile::TempDir,
}

fn a_registry_with(tools: &[(&str, &str)]) -> ARegistry {
    let dir = tempfile::tempdir().unwrap();
    for (name, yaml) in tools {
        std::fs::write(dir.path().join(name), yaml).unwrap();
    }
    let agents = CodingAgentRegistry::load(dir.path()).expect("the registry loads");
    ARegistry {
        agents,
        _tools: dir,
    }
}

fn a_session_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn the_record_in(session_dir: &Path) -> String {
    std::fs::read_to_string(session_dir.join(ANSWER_WAIT_FILE)).expect("a recorded wait")
}

#[test]
fn a_recorded_wait_reads_back_as_recorded() {
    // Given
    let session = a_session_dir();

    // When
    record_answer_wait(session.path(), Duration::from_millis(2_500)).expect("the wait is recorded");

    // Then
    assert_eq!(
        (
            recorded_answer_wait(session.path()),
            the_record_in(session.path())
        ),
        (Some(Duration::from_millis(2_500)), "2500".to_string())
    );
}

#[test]
fn a_session_that_recorded_no_wait_reads_back_none() {
    // Given
    let session = a_session_dir();

    // When
    let recorded = recorded_answer_wait(session.path());

    // Then
    assert_eq!(recorded, None);
}

#[test]
fn the_recorded_wait_is_the_hold_even_when_the_spec_now_says_otherwise() {
    // Given — started while claude.yaml said 7 seconds; the override now says 90
    let registry = a_registry_with(&[("claude.yaml", &claude_yaml_waiting(90))]);
    let session = a_session_dir();
    record_answer_wait(session.path(), Duration::from_secs(7)).expect("the wait is recorded");

    // When
    let hold = session_answer_wait(session.path(), "claude-cli", &registry.agents);

    // Then
    assert_eq!(hold, Duration::from_secs(7));
}

#[test]
fn a_session_with_no_record_is_held_for_its_agent_specs_answer_wait() {
    // Given
    let registry = a_registry_with(&[("claude.yaml", &claude_yaml_waiting(45))]);
    let session = a_session_dir();

    // When
    let hold = session_answer_wait(session.path(), "claude-cli", &registry.agents);

    // Then
    assert_eq!(hold, Duration::from_secs(45));
}

#[test]
fn a_session_of_a_type_no_spec_names_is_held_for_the_specs_default() {
    // Given
    let registry = a_registry_with(&[]);
    let session = a_session_dir();

    // When
    let hold = session_answer_wait(session.path(), "no-such-agent-cli", &registry.agents);

    // Then
    assert_eq!(hold, Duration::from_secs(DEFAULT_ANSWER_WAIT_SECS));
}
