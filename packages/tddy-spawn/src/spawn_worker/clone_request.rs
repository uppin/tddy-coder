//! The worker's clone operation: a git clone run as another OS user.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::clone_run::clone_with_progress;
use crate::git_progress::GitProgress;

/// Request to clone a git repository as an OS user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneRequest {
    pub os_user: String,
    pub git_url: String,
    pub destination: String,
    /// Environment the clone runs with (see `clone_env::CloneSshEnv`). Optional on the wire: a
    /// request from a daemon that predates the field carries none.
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

impl CloneRequest {
    /// Run the clone as the requested OS user, with the request's environment on the `git` child.
    /// `on_started` receives the child's pid once the child is running; `on_progress` each
    /// throttled line of git's progress.
    pub(super) fn run(
        &self,
        on_started: impl FnOnce(u32),
        on_progress: impl FnMut(GitProgress),
    ) -> anyhow::Result<()> {
        let env: Vec<(&str, &str)> = self
            .env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        clone_with_progress(
            &self.os_user,
            &self.git_url,
            Path::new(&self.destination),
            &env,
            |handle| on_started(handle.pid()),
            on_progress,
        )
    }
}
