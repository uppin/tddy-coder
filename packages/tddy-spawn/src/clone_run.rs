//! Running one clone: `git clone --progress` as the target account, into a temporary sibling of the
//! destination that is renamed into place only when git succeeds. Git's stderr is read as it is
//! written and parsed into [`GitProgress`]; the clone can be stopped through its [`CloneHandle`].
//!
//! A destination therefore either does not exist or holds a complete clone — never a half-written
//! one that a later start's "already present" check would mistake for a project.

use std::path::Path;

use crate::git_progress::GitProgress;

/// Stops a running clone: its git process and everything git started (ssh, index-pack), as one
/// process group.
#[derive(Debug, Clone)]
pub struct CloneHandle {
    pid: u32,
}

impl CloneHandle {
    /// A handle on the clone whose process-group leader is `pid`.
    pub fn for_process_group(pid: u32) -> CloneHandle {
        CloneHandle { pid }
    }

    /// The clone's git process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// End the clone. Its temporary directory is removed by the clone that was running.
    pub fn stop(&self) -> std::io::Result<()> {
        // TODO(clone-progress): implement — killpg(SIGTERM), SIGKILL after a grace period
        Err(std::io::Error::other("CloneHandle::stop: not implemented"))
    }
}

/// Clone `git_url` into `destination` as `os_user`, with `env` on the git child. Calls `on_started`
/// once git is running and `on_progress` for each throttled progress line; returns when git does.
/// Skips (Ok) when `destination` already exists.
pub fn clone_with_progress(
    os_user: &str,
    git_url: &str,
    destination: &Path,
    env: &[(&str, &str)],
    on_started: impl FnOnce(CloneHandle),
    on_progress: impl FnMut(GitProgress),
) -> anyhow::Result<()> {
    // TODO(clone-progress): implement over tddy_daemon_kernel::pre_exec_stages::StagedCommand
    let _ = (os_user, git_url, destination, env);
    drop(on_started);
    drop(on_progress);
    anyhow::bail!("clone_with_progress: not implemented")
}
