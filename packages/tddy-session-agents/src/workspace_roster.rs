//! Which agents a `workspace` session's roster may hold — the remote-codebase mode's coding
//! subagents, run on the codebase host itself.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Coding subagents on the codebase host

use tddy_core::AgentKindRecord;
use tddy_rpc::Status;

/// Admit `record` to a workspace session served by `local_daemon_instance_id`: a coding agent this
/// same daemon owns. A model agent, or an agent of another daemon, is refused naming it.
pub fn workspace_roster_admits(
    record: &tddy_core::SessionAgentRecord,
    local_daemon_instance_id: &str,
) -> Result<(), Status> {
    if record.kind != AgentKindRecord::CodingAgent {
        return Err(Status::invalid_argument(format!(
            "{} is a model agent; a workspace session's roster holds coding agents only",
            record.agent_id
        )));
    }
    if record.daemon_instance_id != local_daemon_instance_id {
        return Err(Status::invalid_argument(format!(
            "{} is owned by daemon {}; a workspace session's roster holds only the coding agents \
             of the daemon serving it ({local_daemon_instance_id})",
            record.agent_id, record.daemon_instance_id
        )));
    }
    Ok(())
}
