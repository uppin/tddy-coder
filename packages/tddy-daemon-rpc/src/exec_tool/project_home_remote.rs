//! A project-home session's reach across hosts (`#start-parity` 1/9).
//!
//! Three things, all over the existing peer forward ([`PeerRouting::rpc_served_by_peer`]):
//!
//! - **The target's host.** A target tool call whose arguments name another host
//!   ([`target_daemon_instance_id`]) is re-issued whole to that host, with
//!   `ExecuteToolRequest.daemon_instance_id` set, and its answer returned. The tool runs there
//!   unchanged: that host's containment, activity gate and push identity apply.
//! - **The fan-out list.** `ListProjectSessions` asks every other eligible host for its own
//!   sessions by forwarding the same call; a host that does not answer is named, never dropped.
//! - **The caller, confirmed at its owner.** A peer that receives such a call does not hold the
//!   calling session, so it cannot read its role. It asks every other eligible daemon
//!   `ListSessions` with the caller's token, and serves the call only when the daemon that owns the
//!   session reports it with the project-home role; that row's project is the caller's. A call an
//!   ordinary session's agent addresses at a peer itself (`ExecuteTool` forwards before
//!   authenticating) is refused there, because the owner reports its real role.
//!
//! A call served for a caller held elsewhere is served for this host only: its listing does not
//! fan out and its target is not forwarded again, so a call never travels more than one hop.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::session::{ListSessionsRequest, ListSessionsResponse};
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_tool_engine::project_home_git_tools::is_project_home_git_tool;
use tddy_tool_engine::project_home_tools::{
    is_project_home_target_tool, is_project_home_tool, target_daemon_instance_id,
    ListProjectSessionsResult, UnreachableHost,
};
use tddy_tool_engine::EXEC_TOOL_SERVICE;
use tddy_worktree_service::session_target::HomeCaller;

use super::ExecToolRpcHandler;

const SESSION_SERVICE: &str = "session.SessionService";

impl ExecToolRpcHandler {
    /// Serve a project-home tool call whose calling session this daemon does not hold, once the
    /// daemon that owns it confirms it is a project-home session. `Ok(None)` when the call is not
    /// one: another tool, or a caller held here, which the local path serves.
    ///
    /// For a caller [`authorize_exec_tool_caller`](tddy_session_lifecycle::connection_service::authorize_exec_tool_caller)
    /// has already accepted.
    pub(super) async fn serve_project_home_for_caller_held_elsewhere(
        &self,
        req: &ExecuteToolRequest,
    ) -> Result<Option<ExecuteToolResponse>, Status> {
        if !is_project_home_tool(&req.tool_name) && !is_project_home_git_tool(&req.tool_name) {
            return Ok(None);
        }
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let os_user = self.os_user_of(&req.session_token)?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        if unified_session_dir_path(&sessions_base, &req.session_id).is_dir() {
            return Ok(None);
        }
        let home = self.home_caller_confirmed_at_owner(req).await?;
        if let Some(host) = target_daemon_instance_id(&req.args_json)
            .filter(|host| *host != local_instance_id_for_config(&self.config))
        {
            return Err(Status::failed_precondition(format!(
                "tool {} is refused: a call forwarded from session {}'s host is served only for \
                 this host's sessions, and it names host {host}",
                req.tool_name, req.session_id
            )));
        }
        let port = self.project_home_port(req, os_user, &sessions_base, None)?;
        Ok(Some(port.execute(&home, req).await))
    }

    /// The calling session as the daemon that owns it reports it, when that is a project-home
    /// session; PERMISSION_DENIED otherwise. Every other eligible daemon is asked, and only a row
    /// the owner reports for itself is believed.
    async fn home_caller_confirmed_at_owner(
        &self,
        req: &ExecuteToolRequest,
    ) -> Result<HomeCaller, Status> {
        let local_id = local_instance_id_for_config(&self.config);
        let ask = ListSessionsRequest {
            session_token: req.session_token.clone(),
        };
        let asking: Vec<_> = other_hosts(&self.peer_routing, &local_id)
            .map(|peer| {
                let routing = self.peer_routing.clone();
                let ask = ask.clone();
                tokio::spawn(async move {
                    let answer = routing
                        .rpc_served_by_peer::<_, ListSessionsResponse>(
                            SESSION_SERVICE,
                            "ListSessions",
                            &peer,
                            &ask,
                        )
                        .await;
                    (peer, answer)
                })
            })
            .collect();
        for asked in asking {
            let (peer, answer) = asked.await.map_err(|e| {
                Status::internal(format!("asking a daemon for its sessions failed: {e}"))
            })?;
            let listed = match answer {
                Ok(Some(listed)) => listed,
                Ok(None) => continue,
                Err(status) => {
                    log::info!(
                        "project-home caller {}: daemon {peer} could not be asked for its sessions: {}",
                        req.session_id,
                        status.message()
                    );
                    continue;
                }
            };
            let Some(owned) = listed
                .sessions
                .into_iter()
                .find(|s| s.session_id == req.session_id && s.daemon_instance_id == peer)
            else {
                continue;
            };
            if owned.session_role != PROJECT_HOME_ROLE {
                return Err(Status::permission_denied(format!(
                    "tool {} is served only to a project-home session, and daemon {peer} reports \
                     session {} is not one",
                    req.tool_name, req.session_id
                )));
            }
            return Ok(HomeCaller {
                session_id: owned.session_id,
                project_id: owned.project_id,
            });
        }
        Err(Status::permission_denied(format!(
            "tool {} is refused: no daemon confirms session {} as a project-home session",
            req.tool_name, req.session_id
        )))
    }

    /// The target host's answer to `req`, when it is a target tool call naming a host other than
    /// this one: the same request, addressed at that host.
    pub(super) async fn served_on_target_host(
        &self,
        req: &ExecuteToolRequest,
    ) -> Result<Option<ExecuteToolResponse>, Status> {
        if !is_project_home_target_tool(&req.tool_name) {
            return Ok(None);
        }
        let Some(host) = target_daemon_instance_id(&req.args_json) else {
            return Ok(None);
        };
        let addressed = ExecuteToolRequest {
            daemon_instance_id: host.clone(),
            ..req.clone()
        };
        self.peer_routing
            .rpc_served_by_peer(EXEC_TOOL_SERVICE, "ExecuteTool", &host, &addressed)
            .await
    }
}

/// Every eligible host but `local_id`, in roster order.
fn other_hosts(routing: &PeerRouting, local_id: &str) -> impl Iterator<Item = String> {
    let local_id = local_id.to_string();
    routing
        .eligible_instance_ids()
        .into_iter()
        .filter(move |id| *id != local_id)
}

/// Asks every other eligible host for its sessions of the project, by forwarding the home
/// session's own `ListProjectSessions` call to it.
pub(super) struct PeerSessionLists {
    pub(super) routing: PeerRouting,
    pub(super) local_id: String,
    pub(super) request: ExecuteToolRequest,
}

impl PeerSessionLists {
    /// Every other host's sessions, asked all at once, in roster order.
    pub(super) async fn list(&self) -> ListProjectSessionsResult {
        let asking: Vec<_> = other_hosts(&self.routing, &self.local_id)
            .map(|peer| {
                let routing = self.routing.clone();
                let request = self.request.clone();
                let host = peer.clone();
                (
                    peer,
                    tokio::spawn(async move { ask_for_sessions(&routing, &request, &host).await }),
                )
            })
            .collect();
        let mut result = ListProjectSessionsResult {
            sessions: Vec::new(),
            unreachable_hosts: Vec::new(),
        };
        for (peer, asked) in asking {
            let answer = asked
                .await
                .unwrap_or_else(|e| Err(format!("asking it failed: {e}")));
            match answer {
                Ok(listed) => result.sessions.extend(listed.sessions),
                Err(reason) => {
                    log::info!("ListProjectSessions: host {peer} did not answer: {reason}");
                    result.unreachable_hosts.push(UnreachableHost {
                        daemon_instance_id: peer,
                        reason,
                    });
                }
            }
        }
        result
    }
}

/// `peer`'s own sessions of the project — `request` addressed at it — or why it gave none.
async fn ask_for_sessions(
    routing: &PeerRouting,
    request: &ExecuteToolRequest,
    peer: &str,
) -> Result<ListProjectSessionsResult, String> {
    let addressed = ExecuteToolRequest {
        daemon_instance_id: peer.to_string(),
        ..request.clone()
    };
    let answered: ExecuteToolResponse = routing
        .rpc_served_by_peer(EXEC_TOOL_SERVICE, "ExecuteTool", peer, &addressed)
        .await
        .map_err(|status| status.message().to_string())?
        .ok_or_else(|| format!("host {peer} routes to this daemon"))?;
    if answered.is_error {
        return Err(answered.error_message);
    }
    serde_json::from_str(&answered.result_json)
        .map_err(|e| format!("its answer is not a session list: {e}"))
}
