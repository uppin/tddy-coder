use super::DaemonSessionHost;

use std::path::Path;

use super::super::validate_stack_seed_base_session;

use crate::{connection_service::service_util, user_sessions_path::projects_path_for_user};

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_service::proto::session::StartSessionRequest;

use crate::livekit_peer_discovery::local_instance_id_for_config;

impl DaemonSessionHost {
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
        let inner = tddy_daemon_livekit::livekit_peer_discovery::forward_start_session_via_livekit(
            slot,
            &peer_instance_id,
            req,
        )
        .await?;
        log::info!(
            "StartSession: forward succeeded session_id={} livekit_server_identity={}",
            inner.session_id,
            inner.livekit_server_identity
        );
        Ok(inner)
    }

    pub(super) async fn provision_project_for_start(
        &self,
        req: &StartSessionRequest,
        os_user: &str,
    ) -> Result<(), Status> {
        let project_id = req.project_id.trim();
        if !project_id.is_empty() {
            let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve projects path"))?;
            self.ensure_project_available_for_start(
                os_user,
                &projects_dir,
                project_id,
                &req.session_token,
                req.agent_clone.as_ref(),
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
