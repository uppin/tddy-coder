//! Where a target session's tab runs (`#session-liveness` 6/8): the [`TabTargetShell`] port, which
//! resolves a session's **target** — the machine its `.session.yaml` `target` names — to the shell
//! that reaches it, read with the session owner's credentials, and the session's worktree there
//! (its `repo_path`).
//!
//! The terminal roster (a tab opened "On target") and the roster restore (a `target` → `shell` tab
//! on Resume) both ask it. The daemon's is [`SessionTargetShells`]; a test gives the host a double
//! instead ([`LaunchSessions::tab_target_shell`]).
//!
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs)

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::read_session_metadata;
use tddy_host_service::coder_workspaces::CoderCredentialSource;
use tddy_remote_shell::RemoteShell;
use tddy_rpc::{Code, Status};

use crate::launch_ports::LaunchSessions;
use crate::opencode_cli_spawn::session_start::shell_for_session_target;

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
    pub(crate) coder_credentials: Arc<dyn CoderCredentialSource>,
}

impl SessionTargetShells {
    /// Targets resolved with the Coder accounts `coder_credentials` reads from their owners' vaults.
    pub fn new(coder_credentials: Arc<dyn CoderCredentialSource>) -> Self {
        Self { coder_credentials }
    }
}

impl TabTargetShell for SessionTargetShells {
    fn target_of(&self, session_dir: &Path, subject: &str) -> Result<TabTarget, Status> {
        let session = session_dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let metadata = read_session_metadata(session_dir).map_err(|error| {
            Status::failed_precondition(format!(
                "the record of session {session} could not be read: {error}"
            ))
        })?;
        let target = metadata.target.ok_or_else(|| {
            Status::failed_precondition(format!("session {session} records no target"))
        })?;
        let worktree = metadata.repo_path.ok_or_else(|| {
            Status::failed_precondition(format!(
                "session {session} records no worktree on its target"
            ))
        })?;
        let shell = shell_for_session_target(&target, subject, self.coder_credentials.as_ref())
            .map_err(failed_precondition_of)?;
        Ok(TabTarget {
            shell,
            worktree: PathBuf::from(worktree),
        })
    }
}

/// A target a tab cannot reach — its vault locked, its account missing — is a failed precondition
/// of the tab, whatever way [`shell_for_session_target`] classes the other refusals of a start.
fn failed_precondition_of(refusal: Status) -> Status {
    if refusal.code() == Code::FailedPrecondition {
        return refusal;
    }
    Status::failed_precondition(refusal.message().to_string())
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
