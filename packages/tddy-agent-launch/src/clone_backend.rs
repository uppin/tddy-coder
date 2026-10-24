//! Running one clone on whichever backend the daemon is configured with, carrying the environment
//! the clone was planned with.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use tddy_service::proto::session::CloneProgress;
use tddy_spawn::clone_run::CloneHandle;
use tddy_spawn::git_progress::GitProgress;
use tddy_spawn::spawn_worker::WorkerEvent;
use tddy_spawn::supervisor_client::SpawnBackendChoice;
use tddy_spawn::{clone_run, spawn_worker};

/// A line of git's progress as the start stream carries it.
pub(crate) fn clone_progress_event(progress: GitProgress) -> CloneProgress {
    CloneProgress {
        stage: progress.stage,
        percent: progress.percent,
        objects_done: progress.objects_done,
        objects_total: progress.objects_total,
        bytes_received: progress.bytes_received,
        bytes_per_second: progress.bytes_per_second,
        line: progress.line,
    }
}

/// Everything a clone needs besides the URL, the destination and its environment.
pub(crate) struct CloneBackend {
    pub choice: SpawnBackendChoice,
    pub spawn_client: Option<Arc<spawn_worker::SpawnClient>>,
    pub os_user: String,
    pub runtime: tokio::runtime::Handle,
}

impl CloneBackend {
    /// Clone `git_url` into `dest` as the OS user, with `env` on the `git` child, handing each
    /// throttled line of git's progress to `on_progress`, and the handle that stops the clone to
    /// `on_started` once git runs. Blocks.
    pub(crate) fn clone_repo(
        &self,
        git_url: &str,
        dest: &Path,
        env: &BTreeMap<String, String>,
        on_started: impl FnOnce(CloneHandle),
        mut on_progress: impl FnMut(GitProgress),
    ) -> Result<(), String> {
        match &self.choice {
            SpawnBackendChoice::Supervisor { socket_path } => self
                .runtime
                .block_on(
                    tddy_spawn::supervisor_spawn::clone_repo_via_supervisor_with_progress(
                        socket_path,
                        &self.os_user,
                        git_url,
                        dest,
                        env.clone(),
                        on_started,
                        on_progress,
                    ),
                )
                .map_err(|e| format!("{e:#}")),
            SpawnBackendChoice::ForkedWorker => match &self.spawn_client {
                Some(client) => {
                    let mut on_started = Some(on_started);
                    client
                        .clone_repo_observed(
                            spawn_worker::CloneRequest {
                                os_user: self.os_user.clone(),
                                git_url: git_url.to_string(),
                                destination: dest.display().to_string(),
                                env: env.clone(),
                            },
                            |event| match event {
                                WorkerEvent::Started { pid } => {
                                    if let Some(on_started) = on_started.take() {
                                        on_started(CloneHandle::for_process_group(pid));
                                    }
                                }
                                WorkerEvent::CloneProgress(progress) => on_progress(progress),
                            },
                        )
                        .map_err(|e| e.to_string())
                }
                // No forked worker at all: clone in this process.
                None => {
                    let pairs: Vec<(&str, &str)> =
                        env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                    clone_run::clone_with_progress(
                        &self.os_user,
                        git_url,
                        dest,
                        &pairs,
                        on_started,
                        on_progress,
                    )
                    .map_err(|e| e.to_string())
                }
            },
        }
    }
}
