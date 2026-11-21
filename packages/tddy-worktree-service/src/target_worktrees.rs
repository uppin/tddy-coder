//! The worktree of a session whose agent runs on a **target** — a machine the daemon reaches
//! through a remote shell (`#target-parity` 2/7).
//!
//! The project's clone on this host never holds such a worktree, so the Worktree tab's actions —
//! Clean, Delete, Restore — resolve the target session the request names (by its `worktree_path`,
//! or by `session_id` for Restore) and run on its target, through the shell [`TargetShells`] gives.
//! They never touch this host's clone.
//!
//! The shell comes through a port because the code that builds it (an ssh transport, or a Coder
//! workspace reached with its owner's vault credentials) lives in `tddy-agent-launch`, which
//! depends on this crate.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-21-target-worktree-actions.md.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::SessionTarget;
use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;

/// The shell of a session's target, resolved as the session's owner `os_user`.
#[async_trait]
pub trait TargetShells: Send + Sync {
    /// The shell that reaches `target`. A target that cannot be reached now — a locked Coder
    /// vault, an unknown account — is an error whose message says why.
    async fn shell_for(
        &self,
        target: &SessionTarget,
        os_user: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status>;
}

/// A daemon that reaches no target: every target session's worktree action is refused, saying so.
pub struct NoTargetShells;

#[async_trait]
impl TargetShells for NoTargetShells {
    async fn shell_for(
        &self,
        _target: &SessionTarget,
        _os_user: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status> {
        Err(Status::failed_precondition(
            "this daemon reaches no session target",
        ))
    }
}
