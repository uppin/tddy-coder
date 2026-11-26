//! A target session's tmux as a [`TmuxLocation`] (`#target-reconnect` 2/3): the target adapter
//! over the [`TabTargetShell`] port, and the resolver that names a session record's location.
//!
//! The shell is resolved **per call** through `TabTargetShell::target_of(session_dir, subject)`,
//! so a vault unlocked after the location was built is picked up by the next probe. Attach is
//! `tmux attach-session` through the target login shell, never `new-session -A` (D2): a
//! re-attach never recreates a session or relaunches an agent.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::ReattachSkip;
use tddy_cli_sessions::tmux_location::{HostTmux, SharedTmuxLocation, TmuxClient, TmuxLocation, TmuxUnreachable};

use super::target_tmux_tab::{
    end_target_tmux_session_named, target_tmux_attach_command, target_tmux_session_lives,
};
use super::transport_client;
use crate::tab_target_shell::TabTargetShell;

/// The tmux on a session's target, reached as the session's recorded owner subject.
pub struct TargetTmux {
    session_dir: PathBuf,
    subject: String,
    shells: Arc<dyn TabTargetShell>,
}

impl TargetTmux {
    pub fn new(session_dir: PathBuf, subject: String, shells: Arc<dyn TabTargetShell>) -> Self {
        Self {
            session_dir,
            subject,
            shells,
        }
    }

    /// The session's target, resolved as its owner — per call, so a later unlock is picked up.
    fn target(&self) -> Result<crate::tab_target_shell::TabTarget, TmuxUnreachable> {
        self.shells
            .target_of(&self.session_dir, &self.subject)
            .map_err(|status| TmuxUnreachable(status.message().to_string()))
    }
}

#[async_trait::async_trait]
impl TmuxLocation for TargetTmux {
    fn describe(&self) -> String {
        let session = self
            .session_dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("the tmux on the target of session {session}")
    }

    async fn session_lives(&self, name: &str) -> Result<bool, TmuxUnreachable> {
        let target = self.target()?;
        target_tmux_session_lives(target.shell.as_ref(), name)
            .await
            .map_err(|error| TmuxUnreachable(error.to_string()))
    }

    async fn attach_client(&self, name: &str) -> Result<TmuxClient, TmuxUnreachable> {
        let target = self.target()?;
        let command = target_tmux_attach_command(&target.worktree, name);
        let client = transport_client(target.shell.as_ref(), &command);
        Ok(TmuxClient {
            argv: client.argv,
            env: client.env,
            // The transport's client runs as the daemon, carrying its ssh environment (D9).
            run_as: None,
            exit_code_in_output: client.exit_code_in_output,
        })
    }

    async fn kill(&self, name: &str) -> Result<(), TmuxUnreachable> {
        let target = self.target()?;
        end_target_tmux_session_named(target.shell.as_ref(), name)
            .await
            .map_err(|error| TmuxUnreachable(error.to_string()))
    }
}

/// The tmux location of the session in `session_dir`: [`HostTmux`] as `os_user` for a session
/// that runs here, [`TargetTmux`] as the record's `owner_subject` for one that runs on a target.
///
/// A target session without an `owner_subject` resolves to [`ReattachSkip::NoOwnerSubject`] —
/// the subject is never derived from the OS user (D3). A target its owner cannot be resolved to
/// (a locked vault, a missing account) is [`ReattachSkip::TargetUnreachable`] naming the refusal,
/// asked once here so the reason is the resolver's own.
pub fn tmux_location_of(
    session_dir: &Path,
    os_user: &str,
    shells: &Arc<dyn TabTargetShell>,
) -> Result<SharedTmuxLocation, ReattachSkip> {
    let metadata = tddy_core::read_session_metadata(session_dir)
        .map_err(|_| ReattachSkip::UnreadableRecord)?;
    if metadata.target.is_none() {
        return Ok(Arc::new(HostTmux::new(os_user)));
    }
    let subject = metadata
        .owner_subject
        .as_deref()
        .map(str::trim)
        .filter(|subject| !subject.is_empty())
        .ok_or_else(|| ReattachSkip::NoOwnerSubject)?;
    // Resolved once here so an unresolvable target skips with its reason, not at first probe.
    shells
        .target_of(session_dir, subject)
        .map_err(|status| ReattachSkip::TargetUnreachable(status.message().to_string()))?;
    Ok(Arc::new(TargetTmux::new(
        session_dir.to_path_buf(),
        subject.to_string(),
        Arc::clone(shells),
    )))
}

#[cfg(test)]
#[path = "target_tmux_location_tests.rs"]
mod tests;
