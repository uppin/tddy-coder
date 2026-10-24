//! The worker's clone operation: a git clone run as another OS user.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::spawner;

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
    /// `on_started` receives the child's pid once the child is running.
    pub(super) fn run(&self, on_started: impl FnOnce(u32)) -> anyhow::Result<()> {
        let env: Vec<(&str, &str)> = self
            .env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        spawner::clone_as_user_observed(
            &self.os_user,
            &self.git_url,
            Path::new(&self.destination),
            &env,
            on_started,
        )
    }
}
