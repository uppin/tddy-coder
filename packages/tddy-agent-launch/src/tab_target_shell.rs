//! Where a target session's tab runs (`#session-liveness` 6/8): the [`TabTargetShell`] port, which
//! resolves a session's **target** — the machine its `.session.yaml` `target` names — to the shell
//! that reaches it, read with the session owner's credentials, and the session's worktree there
//! (its `repo_path`).
//!
//! The terminal roster (a tab opened "On target") and the roster restore (a `target` → `shell` tab
//! on Resume) both ask it. The daemon's is [`SessionTargetShells`]; a test gives the host a double
//! instead ([`LaunchSessions::tab_target_shell`]).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_host_service::coder_workspaces::CoderCredentialSource;
use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;

use crate::launch_ports::LaunchSessions;

/// A target session's target, as a tab runs on it.
pub struct TabTarget {
    /// The shell that reaches the session's target.
    pub shell: Arc<dyn RemoteShell>,
    /// The session's worktree on the target, as its record names it.
    pub worktree: PathBuf,
}

/// Resolves the target a session's tabs run on.
pub trait TabTargetShell: Send + Sync {
    /// The target the session in `session_dir` records, reached as `subject` — who the caller's
    /// session token resolved to (the GitHub user), the owner of the vault a Coder account is read
    /// from — and its worktree there.
    ///
    /// Refused `FAILED_PRECONDITION`, naming why, for a session that records no target or no
    /// worktree, and for an owner whose vault is locked (with its remedy) or does not hold the
    /// target's account — a tab cannot start on a target its owner cannot reach.
    fn target_of(&self, session_dir: &Path, subject: &str) -> Result<TabTarget, Status>;
}

/// The daemon's [`TabTargetShell`]: the session's `.session.yaml` read for its `target` and
/// `repo_path`, and the target resolved as a start resolves it
/// (`opencode_cli_spawn::session_start::shell_for_session_target`) — an ssh target's transport, or a
/// Coder workspace's with the account the caller's vault holds.
pub struct SessionTargetShells {
    coder_credentials: Arc<dyn CoderCredentialSource>,
}

impl SessionTargetShells {
    /// Targets resolved with the Coder accounts `coder_credentials` reads from their owners' vaults.
    pub fn new(coder_credentials: Arc<dyn CoderCredentialSource>) -> Self {
        Self { coder_credentials }
    }
}

impl TabTargetShell for SessionTargetShells {
    fn target_of(&self, session_dir: &Path, subject: &str) -> Result<TabTarget, Status> {
        // TODO(target-shell-tab): implement
        let _ = (&self.coder_credentials, session_dir, subject);
        todo!("resolve a session's target for its tab")
    }
}

impl LaunchSessions {
    /// What resolves a session's target for its tabs: the double a test gave the host, or the
    /// daemon's own [`SessionTargetShells`].
    pub fn tab_target_shell(&self) -> Arc<dyn TabTargetShell> {
        match &self.tab_target_shell {
            Some(shells) => Arc::clone(shells),
            None => Arc::new(SessionTargetShells::new(Arc::clone(
                &self.coder_credentials,
            ))),
        }
    }
}

#[cfg(test)]
#[path = "tab_target_shell_tests.rs"]
mod tests;
