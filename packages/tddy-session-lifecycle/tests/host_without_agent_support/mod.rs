//! A daemon host with no copy of the session's agent, for the suites that start or resume a session
//! on a target (`#tab-hardening` 4/4): the agent runs on the target, so the host needs none.
//!
//! Used by both target suites (`remote_target_session_acceptance`, `coder_target_session_acceptance`),
//! so neither test binary carries dead code.

use std::path::Path;

use crate::cli_harness_support::coding_agents_over;

/// Take the agent off the host whose state lives in `sessions_base`: its `PATH` keeps `tddy-tools`
/// and nothing else. `session_type` is the agent's session type (`acme-cli`), whose bare command
/// `session_host` linked into the host's `tool-bin`.
pub fn take_the_agent_off_the_host(sessions_base: &Path, session_type: &str, agent_id: &str) {
    let command = session_type.strip_suffix("-cli").unwrap_or(session_type);
    std::fs::remove_file(sessions_base.join("tool-bin").join(command))
        .expect("the agent installed on the host is removed");
    assert!(
        coding_agents_over(sessions_base)
            .resolve(agent_id, None)
            .is_err(),
        "the daemon's host must not resolve the agent for this test"
    );
}
