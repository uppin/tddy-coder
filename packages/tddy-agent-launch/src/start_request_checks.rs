use std::path::Path;

use tddy_core::{write_session_github_source, SessionGithubSource};

use crate::stack_seed_validation::validate_stack_seed_base_session;

use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_session_split::service_util;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_service::proto::session::{GithubSource, StartSessionRequest};

use crate::launch_ports::LaunchSessions;
use crate::start_keepalive::{with_keepalive, START_KEEPALIVE_INTERVAL};
use tddy_projects::project_storage::ProjectData;
use tddy_session_files::attachment_progress::AttachmentProgressSink;

/// The request a start is forwarded to a peer with: a request that names no GitHub source names
/// this host's own project setting (`local_row`), so the peer acts the same way even if its copy of
/// the project was never set. A request that already names one keeps it; with no local row there is
/// nothing to name.
pub fn forwarded_start_request(
    req: &StartSessionRequest,
    local_row: Option<&ProjectData>,
) -> StartSessionRequest {
    let mut forwarded = req.clone();
    if req.github_source() == GithubSource::Unspecified {
        if let Some(row) = local_row {
            forwarded.set_github_source(if row.local_gh {
                GithubSource::LocalGh
            } else {
                GithubSource::Vault
            });
        }
    }
    forwarded
}
/// Record the GitHub source a start named in the new session's directory, so its resumes act the
/// same way; a start that named none records nothing, and its resumes read this host's row again.
pub(crate) fn record_requested_github_source(
    req: &StartSessionRequest,
    session_dir: &Path,
) -> Result<(), Status> {
    let source = match req.github_source() {
        GithubSource::Unspecified => return Ok(()),
        GithubSource::Vault => SessionGithubSource::Vault,
        GithubSource::LocalGh => SessionGithubSource::LocalGh,
    };
    std::fs::create_dir_all(session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
    write_session_github_source(session_dir, source)
        .map_err(|e| Status::internal(format!("failed to record the session's GitHub source: {e}")))
}

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

/// How long a forwarded `StartSession` waits for the peer's answer.
///
/// The peer may clone the project first, and a clone over SSH can stop to ask its operator for a key
/// passphrase (up to the prompt's lifetime), so the ordinary peer-forward deadline is too short. A
/// split start already waits the peer's whole clone budget for the same peer-side work; this is that
/// one budget.
#[must_use]
pub fn start_forward_deadline(
    config: &tddy_daemon_kernel::config::DaemonConfig,
) -> std::time::Duration {
    tddy_session_agents::agent_roster::split_forward_deadline(config)
}

impl LaunchSessions {
    pub(super) fn eligible_daemon_ids(&self) -> (String, Vec<String>) {
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_rows = self
            .peer_routing
            .eligible_daemon_source()
            .list_eligible_daemons();
        let eligible_ids: Vec<String> = eligible_rows
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        (local_id, eligible_ids)
    }

    pub(super) async fn forward_start_session(
        &self,
        req: &StartSessionRequest,
        peer_instance_id: String,
    ) -> Result<StartSessionResponse, Status> {
        log::info!(
            "StartSession: forwarding RPC to remote daemon_instance_id={}",
            peer_instance_id
        );
        let slot = self.peer_routing.common_room_livekit_room().ok_or_else(|| {
                Status::failed_precondition(
                    "cannot forward StartSession: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)",
                )
            })?;
        let inner = tddy_daemon_kernel::peer_calls::forward_start_session_within(
            slot,
            &peer_instance_id,
            req,
            start_forward_deadline(&self.config),
        )
        .await?;
        log::info!(
            "StartSession: forward succeeded session_id={} livekit_server_identity={}",
            inner.session_id,
            inner.livekit_server_identity
        );
        Ok(inner)
    }

    /// Forward a start to a peer, naming the GitHub source this host's own row of the project gives
    /// it: the peer may hold no (or a stale) copy of the project. No row here: nothing to name.
    pub(super) async fn forward_start_naming_local_source(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        peer_instance_id: String,
    ) -> Result<StartSessionResponse, Status> {
        let local_row = service_util::find_registered_project(
            &self.tddy_data_dir,
            os_user,
            req.project_id.trim(),
        )
        .ok()
        .map(|(_, project)| project);
        let forwarded = forwarded_start_request(req, local_row.as_ref());
        self.forward_start_session(&forwarded, peer_instance_id)
            .await
    }

    pub(super) async fn provision_project_for_start(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        progress: &AttachmentProgressSink,
    ) -> Result<(), Status> {
        let project_id = req.project_id.trim();
        if !project_id.is_empty() {
            let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve projects path"))?;
            // A clone can wait on the operator's passphrase far longer than a forwarded stream
            // tolerates silence, so the stream is kept alive for as long as this waits.
            with_keepalive(
                progress,
                START_KEEPALIVE_INTERVAL,
                self.ensure_project_available_for_start(
                    os_user,
                    &projects_dir,
                    project_id,
                    &req.session_token,
                    req.agent_clone.as_ref(),
                ),
            )
            .await?;
        }
        Ok(())
    }

    pub(super) fn validate_stack_seed_against_project(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
        sessions_base: std::path::PathBuf,
        project_id: &str,
    ) -> Result<(), Status> {
        let (_, project) =
            service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id)?;
        validate_stack_seed_base_session(
            &sessions_base,
            &req.recipe,
            &req.pr_stack_base_session_id,
            Path::new(&project.main_repo_path),
        )
        .map_err(tddy_service::to_rpc_status)?;
        Ok(())
    }
}
