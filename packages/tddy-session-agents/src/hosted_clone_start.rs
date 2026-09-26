use std::sync::Arc;

use tddy_daemon_livekit::livekit_peer_discovery::local_instance_id_for_config;

use tddy_projects::project_storage;
use tddy_rpc::Status;

use std::path::PathBuf;

#[allow(clippy::too_many_arguments)]
pub fn start_hosted_agent_clone(
    placement: &tddy_service::proto::session::AgentClonePlacement,
    codebase_session_id: &str,
    project_id: &str,
    session_token: &str,
    session_id: &str,
    facilitating: &str,
    url: String,
    api_key: String,
    api_secret: String,
    worktree_path: PathBuf,
    projects_dir: PathBuf,
    state: crate::AgentRosterState<'_>,
) -> Result<(), Status> {
    let project = project_storage::find_project(&projects_dir, project_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| {
            Status::not_found(format!(
                "project '{project_id}' is not registered here, so an agent clone of it has \
                     nothing to fetch the session's WIP ref from"
            ))
        })?;

    // Only a checkout that was cloned *from the facilitating daemon* fetches its WIP ref over
    // `tddy-remote-git-repo` — its `origin` is the facilitator's `{instance_id}:{project_id}` URL,
    // the only place that ref lives. A checkout the owning daemon already had on the shared
    // filesystem fetches the ref from that local repo directly (the facilitating daemon
    // published it there), so it must NOT carry the transport-shim env var: `origin` there is the
    // forge URL, which `tddy-remote-git-repo` would try to reach and fail. (PRD AC37.)
    let facilitator_origin_prefix = format!("{facilitating}:");
    let is_facilitator_clone = project.git_url.starts_with(&facilitator_origin_prefix);

    let spec = crate::session_agent_clone::CloneMirrorSpec {
        session_id: session_id.to_string(),
        facilitating_daemon_instance_id: facilitating.to_string(),
        owning_daemon_instance_id: local_instance_id_for_config(state.config),
        codebase_session_id: codebase_session_id.to_string(),
        worktree_path,
        project_repo_path: PathBuf::from(&project.main_repo_path),
        project_id: project_id.to_string(),
        session_token: session_token.to_string(),
        livekit_url: url,
        livekit_api_key: api_key,
        livekit_api_secret: api_secret,
        facilitating_daemon_url: if is_facilitator_clone {
            let u = placement.facilitating_daemon_url.trim();
            if u.is_empty() {
                None
            } else {
                Some(u.to_string())
            }
        } else {
            None
        },
        first_admission_token: placement.first_admission_token.clone(),
        first_admission_url: placement.first_admission_url.clone(),
        first_admission_room: placement.first_admission_room.clone(),
        common_room_slot: state.peer_routing.common_room_livekit_room().cloned(),
    };
    let hosted = Arc::clone(state.hosted_agent_clones);
    let clone_id = codebase_session_id.to_string();
    tokio::spawn(async move {
        if let Err(status) = crate::session_agent_clone::run_clone_mirror(spec, hosted).await {
            // Loud and final: the facilitating daemon has already been told the clone failed
            // (the mirror reports before it returns), and there is nothing here that could
            // repair a room this daemon cannot reach.
            log::error!(
                "agent clone {clone_id} stopped mirroring: {}",
                status.message()
            );
        }
    });
    Ok(())
}
