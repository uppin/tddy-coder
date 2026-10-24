//! Which agents a `workspace` session's roster may hold — the remote-codebase mode's coding
//! subagents, run on the codebase host itself.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-remote-codebase.md

use tddy_rpc::Status;

/// Admit `record` to a workspace session served by `local_daemon_instance_id`: a coding agent this
/// same daemon owns. A model agent, or an agent of another daemon, is refused naming it.
pub fn workspace_roster_admits(
    _record: &tddy_core::SessionAgentRecord,
    _local_daemon_instance_id: &str,
) -> Result<(), Status> {
    // TODO(remote-codebase): implement
    Err(Status::unimplemented(
        "TODO(remote-codebase): workspace_roster_admits is not implemented",
    ))
}
