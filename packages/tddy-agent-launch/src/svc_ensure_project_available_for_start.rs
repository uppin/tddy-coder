use crate::clone_backend::CloneBackend;
use crate::clone_planner::ClonePlanner;
use crate::launch_ports::LaunchSessions;
use std::path::Path;
use std::path::PathBuf;
use tddy_daemon_kernel::user_paths::repos_base_for_user;
use tddy_projects::project_storage;
use tddy_rpc::Status;
use tddy_spawn::spawn_worker;

/// What provisioning a project's working copy on the blocking pool needs: the clone backend, and
/// where the project comes from.
pub(super) struct ProjectClone {
    repos_base_dir: Option<PathBuf>,
    spawn_client: Option<std::sync::Arc<spawn_worker::SpawnClient>>,
    os_user_owned: String,
    projects_dir_owned: PathBuf,
    project_id_owned: String,
    peer_entries: Vec<tddy_service::proto::project::ProjectEntry>,
    spawn_backend: tddy_spawn::supervisor_client::SpawnBackendChoice,
    runtime: tokio::runtime::Handle,
    planner: ClonePlanner,
    facilitating_remote_url: Option<String>,
}

pub(super) fn spawn_project_clone(
    launch: ProjectClone,
) -> tokio::task::JoinHandle<Result<project_storage::ProjectData, Status>> {
    let ProjectClone {
        repos_base_dir,
        spawn_client,
        os_user_owned,
        projects_dir_owned,
        project_id_owned,
        peer_entries,
        spawn_backend,
        runtime,
        planner,
        facilitating_remote_url,
    } = launch;
    let handle = tokio::task::spawn_blocking(move || {
        let backend = CloneBackend {
            choice: spawn_backend,
            spawn_client,
            os_user: os_user_owned,
            runtime,
        };
        let cloner = |git_url: &str, dest: &Path| -> Result<(), String> {
            // The plan (and the grant it holds) lives until this clone has ended.
            let planned = planner.plan(git_url)?;
            backend
                .clone_repo(git_url, dest, planned.env())
                .map_err(|e| planned.explain_failure(e))
        };
        let provisioned = if let Some(remote_url) = facilitating_remote_url {
            tddy_projects::project_provision::ensure_project_available_from_facilitator(
                &projects_dir_owned,
                &project_id_owned,
                repos_base_dir.as_deref(),
                &remote_url,
                cloner,
            )
        } else {
            let peer_lookup = |id: &str| {
                peer_entries
                    .iter()
                    .find(|p| p.project_id == id)
                    .map(|p| (p.name.clone(), p.git_url.clone()))
            };
            tddy_projects::project_provision::ensure_project_available_locally(
                &projects_dir_owned,
                &project_id_owned,
                repos_base_dir.as_deref(),
                cloner,
                peer_lookup,
            )
        };
        // A clone refused before it started says why, with the status that fits.
        provisioned.map_err(|status| planner.take_failure().unwrap_or(status))
    });
    handle
}
impl LaunchSessions {
    /// Ensure the project's working copy exists on this (local) host before a session starts,
    /// auto-cloning it when missing: from the local registry's `git_url` if the project is already
    /// registered here, otherwise from a peer daemon that hosts it (reusing the logical
    /// `project_id`). The blocking clone runs off the async runtime; a project unknown locally and
    /// on every peer surfaces as `NotFound`.
    ///
    /// When `agent_clone` is set, this daemon is the **owning** side of an agent clone and the
    /// project is provisioned from the facilitating daemon's `remote_git.RemoteGitService` rather
    /// than from a peer-discovered forge URL — `git clone {facilitating_instance_id}:{project_id}`
    /// with `GIT_SSH_COMMAND=tddy-remote-git-repo --daemon-url {facilitating_daemon_url}
    /// --session-token {session_token}`. That closes the two cases the peer fan-out fails for a
    /// remote owning daemon: a peer with no common room of its own, and a project whose `git_url`
    /// names a forge the peer cannot reach (PRD AC37).
    pub(crate) async fn ensure_project_available_for_start(
        &self,
        os_user: &str,
        projects_dir: &Path,
        project_id: &str,
        session_token: &str,
        agent_clone: Option<&tddy_service::proto::session::AgentClonePlacement>,
    ) -> Result<project_storage::ProjectData, Status> {
        // Resolved lazily: only a peer-provisioned clone needs a base path. A locally-registered
        // project (the common case) never consults it, so a `None` here must not fail the start —
        // it only surfaces as an error if `ensure_project_available_locally` actually needs it.
        let repos_base_dir = repos_base_for_user(os_user, self.config.repos_base_path_or_default());
        let spawn_client = self.spawn_client.clone();
        let os_user_owned = os_user.to_string();
        let projects_dir_owned = projects_dir.to_path_buf();
        let project_id_owned = project_id.to_string();
        let timeout = self.config.spawn_worker_request_timeout();

        // An agent clone provisions from its facilitating daemon directly, so it never fans out
        // `ListProjects` across the common room — the fan-out is the path that fails for a peer
        // with no room of its own, and it is the one AC37 replaces.
        let facilitating = match agent_clone {
            Some(placement) => {
                let facilitating_instance_id = placement.facilitating_daemon_instance_id.trim();
                let facilitating_daemon_url = placement.facilitating_daemon_url.trim();
                if facilitating_instance_id.is_empty() || facilitating_daemon_url.is_empty() {
                    return Err(Status::invalid_argument(format!(
                        "agent_clone placement is incomplete: facilitating_daemon_instance_id and \
                         facilitating_daemon_url are both required to provision a clone on a daemon \
                         that has never seen the project (got instance_id={facilitating_instance_id:?}, \
                         url={facilitating_daemon_url:?})"
                    )));
                }
                Some((
                    facilitating_instance_id.to_string(),
                    facilitating_daemon_url.to_string(),
                ))
            }
            None => None,
        };

        // Peer discovery is an async RPC fan-out; resolve it here rather than inside the blocking
        // clone task. It is only needed when the project is not registered on this host — a
        // locally-registered project clones from its stored `git_url` with no peer lookup — so the
        // fan-out is skipped entirely in that case. An agent clone skips the fan-out unconditionally
        // (it provisions from its facilitator, above).
        let registered_locally = project_storage::find_project(projects_dir, project_id)
            .map_err(|e| Status::internal(format!("read project registry: {e}")))?
            .is_some();
        let peer_entries = if registered_locally || facilitating.is_some() {
            Vec::new()
        } else {
            self.peer_routing
                .eligible_daemon_source()
                .peer_project_entries(session_token)
                .await
        };

        let spawn_backend = self.spawn_backend.clone();
        // `ensure_project_available_locally` clones synchronously while the supervisor's client is
        // async. Handing the closure a runtime handle keeps that seam here: the closure already runs
        // on a blocking thread, which is precisely where awaiting a future by blocking belongs.
        let runtime = tokio::runtime::Handle::current();

        // The transport-shim env var a facilitator-provisioned clone sets on the `git clone` child.
        // `tddy-remote-git-repo` takes its daemon URL and session token as `--long` flags on the
        // `GIT_SSH_COMMAND` string, so a single env var carries both — the supervisor's
        // `allowed_env_keys` needs only `GIT_SSH_COMMAND` to admit it.
        let ssh_command = facilitating.as_ref().map(|(_, daemon_url)| {
            format!(
                "{} --daemon-url {daemon_url} --session-token {session_token}",
                tddy_projects::project_provision::resolve_remote_git_repo_path()
            )
        });
        let facilitating_remote_url = facilitating
            .as_ref()
            .map(|(instance_id, _)| format!("{instance_id}:{project_id_owned}"));

        let handle = spawn_project_clone(ProjectClone {
            planner: ClonePlanner::for_launch(self, ssh_command, &os_user_owned, session_token),
            repos_base_dir,
            spawn_client,
            os_user_owned,
            projects_dir_owned,
            project_id_owned,
            peer_entries,
            spawn_backend,
            runtime,
            facilitating_remote_url,
        });

        match tokio::time::timeout(timeout, handle).await {
            Ok(Ok(res)) => res.inspect_err(|status| {
                log::warn!(
                    "ensure_project_available_for_start: clone of project {project_id} failed: {status}"
                )
            }),
            Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => {
                log::warn!(
                    "ensure_project_available_for_start: clone of project {project_id} exceeded its {timeout:?} deadline"
                );
                Err(Status::deadline_exceeded(
                    "ensure_project_available_for_start: clone timed out",
                ))
            }
        }
    }
}
