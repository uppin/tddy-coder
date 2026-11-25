//! The notification relay on this daemon (n5, cross-host watches and messages): this host's
//! [`NotificationRelay`] sending over the existing peer forward, and the peer-only receiving end.
//!
//! - **Sending.** [`ExecToolRpcHandler::start_notification_relay`] runs the relay of
//!   `tddy_session_activity::notification_relay` over [`DaemonRelayPorts`]: a batch is an
//!   `ExecuteTool` call addressed at the receiving host under the internal name
//!   [`RELAY_SESSION_NOTIFICATIONS`], forwarded through [`PeerRouting::rpc_served_by_peer`] exactly
//!   as a target tool call is. No new transport.
//! - **Receiving.** [`ExecToolRpcHandler::serve_relay_call`] answers the two internal names. They
//!   are never tools: no listing offers them, and a call is served only when it arrived from a peer
//!   over the common room, addressed at this host, carrying no session and no token (a relay call
//!   never does), with a body that is a relay request. Everything else is refused — an agent's call
//!   is refused on its own host, before any forward. A relayed record is accepted only when it is
//!   relayed for a session this host holds in the batch's project; it is then appended to this
//!   host's own project log, so its watches, inboxes and wakes see it as their own.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions-on-other-hosts

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::{Request, RequestTransport, Status};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::notification_relay::{
    FollowRequest, NotificationRelay, RelayAck, RelayPorts, RelayRequest,
    FOLLOW_SESSION_NOTIFICATIONS, RELAY_SESSION_NOTIFICATIONS,
};
use tddy_session_activity::project_notification_log::{record_from_json, RelayedBatch};
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_tool_engine::EXEC_TOOL_SERVICE;
use tokio::task::JoinHandle;

use super::watch_wake::WakeCue;
use super::ExecToolRpcHandler;

/// How the relay paces itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayTimings {
    /// How soon a host that did not take a batch is tried again.
    pub retry: Duration,
}

impl Default for RelayTimings {
    /// A host that did not take its batch is tried again after 5 s.
    fn default() -> Self {
        Self {
            retry: Duration::from_secs(5),
        }
    }
}

/// [`RelayPorts`] over this daemon: its identity, its users, and the peer forward.
struct DaemonRelayPorts {
    config: DaemonConfig,
    tddy_data_dir: PathBuf,
    routing: PeerRouting,
}

#[async_trait]
impl RelayPorts for DaemonRelayPorts {
    fn local_instance_id(&self) -> String {
        local_instance_id_for_config(&self.config)
    }

    fn github_user_of(&self, os_user: &str) -> Option<String> {
        self.config
            .users
            .mapping_for_os_user(os_user)
            .map(|mapping| mapping.github_user)
    }

    fn sessions_base_for(&self, os_user: &str) -> Option<PathBuf> {
        sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
    }

    async fn relay(&self, host: &str, request: &RelayRequest) -> Result<RelayAck, String> {
        let answer = call_peer(&self.routing, host, RELAY_SESSION_NOTIFICATIONS, request).await?;
        serde_json::from_str(&answer.result_json)
            .map_err(|e| format!("host {host} answered no relay acknowledgement: {e}"))
    }
}

/// Ask `host` to relay one of its sessions' records to this host for a watch here ([`FollowRequest`]).
pub(super) async fn follow_on(
    routing: &PeerRouting,
    host: &str,
    request: &FollowRequest,
) -> Result<(), String> {
    call_peer(routing, host, FOLLOW_SESSION_NOTIFICATIONS, request)
        .await
        .map(|_| ())
}

/// `body` as an internal relay call to `host` over the peer forward: its answer, or why there was
/// none.
async fn call_peer(
    routing: &PeerRouting,
    host: &str,
    tool_name: &str,
    body: &impl serde::Serialize,
) -> Result<ExecuteToolResponse, String> {
    let request = ExecuteToolRequest {
        session_token: String::new(),
        session_id: String::new(),
        tool_name: tool_name.to_string(),
        args_json: serde_json::to_string(body).map_err(|e| format!("serialize: {e}"))?,
        daemon_instance_id: host.to_string(),
        conversation_id: String::new(),
    };
    let answer: ExecuteToolResponse = routing
        .rpc_served_by_peer(EXEC_TOOL_SERVICE, "ExecuteTool", host, &request)
        .await
        .map_err(|status| status.message().to_string())?
        .ok_or_else(|| format!("host {host} routes to this daemon"))?;
    if answer.is_error {
        return Err(answer.error_message);
    }
    Ok(answer)
}

impl ExecToolRpcHandler {
    /// Start relaying this host's notification-log records to the hosts they concern, keeping the
    /// relay's cursors under `state_dir`. A host with no notification log relays nothing; a relay
    /// whose state cannot be read does not start, and says so in the log.
    pub fn start_notification_relay(
        self: &Arc<Self>,
        state_dir: PathBuf,
        timings: RelayTimings,
    ) -> Option<JoinHandle<()>> {
        let log = self.project_home_host.notification_log.clone()?;
        let ports = Arc::new(DaemonRelayPorts {
            config: self.config.clone(),
            tddy_data_dir: self.tddy_data_dir.clone(),
            routing: self.peer_routing.clone(),
        });
        let relay = match NotificationRelay::open(log, ports, &state_dir) {
            Ok(relay) => Arc::new(relay),
            Err(e) => {
                log::error!("the notification relay did not start: {e:#}");
                return None;
            }
        };
        if self
            .project_home_host
            .relay
            .set(Arc::clone(&relay))
            .is_err()
        {
            log::error!("the notification relay was started twice; keeping the first");
            return None;
        }
        Some(tokio::spawn(relay.run(timings.retry)))
    }

    /// Answer a call of one of the relay's internal names — served only as a peer's relay call.
    pub(super) async fn serve_relay_call(
        &self,
        request: Request<ExecuteToolRequest>,
    ) -> Result<ExecuteToolResponse, Status> {
        let arrived_from_a_peer = request.metadata().transport() == RequestTransport::LiveKit;
        let req = request.into_inner();
        if !arrived_from_a_peer {
            return Err(Status::permission_denied(format!(
                "{} is not a tool: it is answered only to another daemon over the common room",
                req.tool_name
            )));
        }
        let local = local_instance_id_for_config(&self.config);
        let addressed_here = req.daemon_instance_id.is_empty() || req.daemon_instance_id == local;
        if !addressed_here || !req.session_id.is_empty() || !req.session_token.is_empty() {
            return Err(Status::permission_denied(format!(
                "{} is refused: the call is not a relay call addressed at this host",
                req.tool_name
            )));
        }
        let result_json = if req.tool_name == RELAY_SESSION_NOTIFICATIONS {
            let relay: RelayRequest = serde_json::from_str(&req.args_json)
                .map_err(|e| Status::invalid_argument(format!("not a relay request: {e}")))?;
            serde_json::to_string(&self.take_relayed(relay).await?)
        } else {
            let follow: FollowRequest = serde_json::from_str(&req.args_json)
                .map_err(|e| Status::invalid_argument(format!("not a follow request: {e}")))?;
            self.take_follow(&follow)?;
            Ok("{}".to_string())
        }
        .map_err(|e| Status::internal(format!("serialize the answer: {e}")))?;
        Ok(ExecuteToolResponse {
            result_json,
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        })
    }

    /// The OS user `github_user` maps to here, and where its sessions live.
    fn relayed_user(&self, github_user: &str) -> Result<(String, PathBuf), Status> {
        let os_user = self.config.os_user_for_github(github_user).ok_or_else(|| {
            Status::permission_denied(format!(
                "GitHub user {github_user} is not mapped to an OS user on this host"
            ))
        })?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        Ok((os_user, sessions_base))
    }

    /// Append what a peer relayed to this host's own log, keeping only the records relayed for a
    /// session this host holds in the batch's project, and wake the watching orchestrators.
    async fn take_relayed(&self, relay: RelayRequest) -> Result<RelayAck, Status> {
        let log = self
            .project_home_host
            .notification_log
            .clone()
            .ok_or_else(|| Status::failed_precondition("this host keeps no notification log"))?;
        let (os_user, sessions_base) = self.relayed_user(&relay.github_user)?;
        let mut records = Vec::with_capacity(relay.records.len());
        let mut anchors = std::collections::BTreeSet::new();
        for relayed in relay.records {
            if !holds_session_of_project(
                &sessions_base,
                &relayed.anchor_session_id,
                &relay.project_id,
            ) {
                log::warn!(
                    "host {} relayed a record for session {}, which this host does not hold in project {}; dropped",
                    relay.origin_daemon_instance_id,
                    relayed.anchor_session_id,
                    relay.project_id
                );
                continue;
            }
            let record = record_from_json(&relayed.record_json).map_err(|e| {
                Status::invalid_argument(format!("a relayed record is not a log record: {e}"))
            })?;
            anchors.insert(relayed.anchor_session_id);
            records.push(record);
        }
        let appended = log
            .append_relayed(RelayedBatch {
                origin: relay.origin_daemon_instance_id,
                project_id: relay.project_id.clone(),
                os_user: os_user.clone(),
                records,
                lost: relay.lost,
                through_seq: relay.through_seq,
            })
            .map_err(|e| Status::internal(format!("append the relayed records: {e:#}")))?;
        if !appended.appended.is_empty() {
            if let Some(cues) = self.project_home_host.relayed_cues.get() {
                for anchor in anchors {
                    // A full channel drops the cue; the next event looks again.
                    let _ = cues.try_send(WakeCue {
                        session_id: anchor,
                        os_user: os_user.clone(),
                        project_id: Some(relay.project_id.clone()),
                    });
                }
            }
        }
        Ok(RelayAck {
            received_through_seq: appended.received_through,
        })
    }

    /// Register a peer's watch as following one of this host's sessions.
    fn take_follow(&self, follow: &FollowRequest) -> Result<(), Status> {
        let relay = self.project_home_host.relay.get().ok_or_else(|| {
            Status::failed_precondition("this host does not relay its sessions' notifications")
        })?;
        let (_, sessions_base) = self.relayed_user(&follow.github_user)?;
        if !holds_session_of_project(&sessions_base, &follow.session_id, &follow.project_id) {
            return Err(Status::not_found(format!(
                "session {} is not a session of project {} on this host",
                follow.session_id, follow.project_id
            )));
        }
        relay.follow(follow);
        Ok(())
    }
}

/// Whether `session_id` has a record under `sessions_base` naming `project_id`.
fn holds_session_of_project(
    sessions_base: &std::path::Path,
    session_id: &str,
    project_id: &str,
) -> bool {
    validate_session_id_segment(session_id).is_ok()
        && tddy_core::read_session_metadata(&unified_session_dir_path(sessions_base, session_id))
            .is_ok_and(|metadata| metadata.project_id == project_id)
}
