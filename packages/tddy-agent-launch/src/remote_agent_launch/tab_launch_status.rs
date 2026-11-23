//! The one status a target tab's failed launch reports (`#tab-hardening` 1/4): shell tabs and agent
//! tabs alike, so a client reads one meaning from one code whichever kind it opened.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-tab-errors.md

use tddy_rpc::Status;

use super::RemoteLaunchError;
use crate::target_trust::TargetTrustError;

/// What a target tab's failed launch says to the caller, the error's own words as the message:
///
/// - [`RemoteLaunchError::Shell`] — the target resolved but its transport did not reach it, or a
///   command there (entering the worktree) failed — is `UNAVAILABLE`: the target, not the request,
///   is at fault;
/// - [`RemoteLaunchError::Spawn`] — this host could not spawn the tab's local terminal — is
///   `INTERNAL`: a local resource problem, not a precondition the caller can meet;
/// - [`RemoteLaunchError::AgentNotOnTarget`] — the target's login shell does not find the agent —
///   is `FAILED_PRECONDITION`, naming the command;
/// - anything else the launch refuses is `FAILED_PRECONDITION`.
///
/// A target that cannot be resolved (a locked vault, a missing account) never reaches this mapping:
/// the target-shell port refuses it `FAILED_PRECONDITION` before any launch.
pub fn tab_launch_status(error: &RemoteLaunchError) -> Status {
    let message = error.to_string();
    match error {
        RemoteLaunchError::Shell(_) | RemoteLaunchError::Trust(TargetTrustError::Shell(_)) => {
            Status::unavailable(message)
        }
        RemoteLaunchError::Spawn(_) => Status::internal(message),
        RemoteLaunchError::AgentNotOnTarget { .. }
        | RemoteLaunchError::TmuxNotOnTarget
        | RemoteLaunchError::Plan(_)
        | RemoteLaunchError::Trust(TargetTrustError::Malformed { .. })
        | RemoteLaunchError::ExitedDuringStartup { .. }
        | RemoteLaunchError::AgentNeverStarted { .. }
        | RemoteLaunchError::AgentNeverAnswered { .. } => Status::failed_precondition(message),
    }
}

#[cfg(test)]
#[path = "tab_launch_status_tests.rs"]
mod tests;
