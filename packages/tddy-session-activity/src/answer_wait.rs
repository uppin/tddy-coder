//! How long `AwaitQuestionAnswer` holds one session's hook (`#agent-tools 19/19`).
//!
//! The hold must equal the hook timeout the agent was launched with: a launch writes the hooks file
//! with `timeout = answer_wait_secs + margin` from its agent's spec
//! (`tddy_agent_backend::claude_hooks`), and a hold longer than that would be killed by the agent
//! itself. So the wait is **recorded in the session directory at start** — beside `.session.yaml`,
//! in [`ANSWER_WAIT_FILE`] — and the hold reads that record, never a since-edited override of the
//! spec. A session with no record (one started before this file existed) is held for its agent
//! spec's `coding_agent.answer_wait_secs`; a session type no spec names, for
//! [`tddy_coding_agent_specs::DEFAULT_ANSWER_WAIT_SECS`]. There is no daemon-wide value.
//!
//! The record is a file of its own rather than a `SessionMetadata` field: `.session.yaml` is
//! `deny_unknown_fields`, so a new key would make a downgraded daemon drop every session started by
//! this one.

use std::path::Path;
use std::time::Duration;

use tddy_coding_agent_specs::{CodingAgentRegistry, DEFAULT_ANSWER_WAIT_SECS};

/// The file in a session directory holding the wait its start recorded, in milliseconds.
pub const ANSWER_WAIT_FILE: &str = "answer-wait-ms";

/// Record `wait` as the answer wait of the session in `session_dir` — what its launch wrote into
/// the agent's hooks file.
///
/// # Errors
/// When the record cannot be written.
pub fn record_answer_wait(session_dir: &Path, wait: Duration) -> std::io::Result<()> {
    std::fs::write(
        session_dir.join(ANSWER_WAIT_FILE),
        wait.as_millis().to_string(),
    )
}

/// The answer wait the start of the session in `session_dir` recorded; `None` when it recorded
/// none (or the record does not parse).
#[must_use]
pub fn recorded_answer_wait(session_dir: &Path) -> Option<Duration> {
    let text = std::fs::read_to_string(session_dir.join(ANSWER_WAIT_FILE)).ok()?;
    let millis: u64 = text.trim().parse().ok()?;
    Some(Duration::from_millis(millis))
}

/// How long the hook of the session in `session_dir`, of `session_type`, is held: its recorded
/// wait, else its agent spec's `coding_agent.answer_wait_secs`, else
/// [`tddy_coding_agent_specs::DEFAULT_ANSWER_WAIT_SECS`].
#[must_use]
pub fn session_answer_wait(
    session_dir: &Path,
    session_type: &str,
    agents: &CodingAgentRegistry,
) -> Duration {
    recorded_answer_wait(session_dir).unwrap_or_else(|| {
        Duration::from_secs(
            agents
                .for_session_type(session_type)
                .map_or(DEFAULT_ANSWER_WAIT_SECS, |spec| spec.answer_wait_secs()),
        )
    })
}
