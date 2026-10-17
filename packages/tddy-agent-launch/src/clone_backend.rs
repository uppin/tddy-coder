//! Running one clone on whichever backend the daemon is configured with, carrying the environment
//! the clone was planned with.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_spawn::supervisor_client::SpawnBackendChoice;
use tddy_spawn::{spawn_worker, spawner};

/// Everything a clone needs besides the URL, the destination and its environment.
pub(crate) struct CloneBackend {
    pub choice: SpawnBackendChoice,
    pub spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    pub os_user: String,
    pub runtime: tokio::runtime::Handle,
}

impl CloneBackend {
    /// Clone `git_url` into `dest` as the OS user, with `env` on the `git` child. Blocks.
    pub(crate) fn clone_repo(
        &self,
        git_url: &str,
        dest: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        match &self.choice {
            SpawnBackendChoice::Supervisor { socket_path } => self
                .runtime
                .block_on(
                    tddy_spawn::supervisor_spawn::clone_repo_via_supervisor_with_env(
                        socket_path,
                        &self.os_user,
                        git_url,
                        dest,
                        env.clone(),
                    ),
                )
                .map_err(|e| format!("{e:#}")),
            SpawnBackendChoice::ForkedWorker => match &self.spawn_client {
                Some(client) => client
                    .clone_repo(spawn_worker::CloneRequest {
                        os_user: self.os_user.clone(),
                        git_url: git_url.to_string(),
                        destination: dest.display().to_string(),
                        env: env.clone(),
                    })
                    .map_err(|e| e.to_string()),
                // No forked worker at all: clone in this process.
                None => {
                    let pairs: Vec<(&str, &str)> =
                        env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                    spawner::clone_as_user_with_env(&self.os_user, git_url, dest, &pairs)
                        .map_err(|e| e.to_string())
                }
            },
        }
    }
}
