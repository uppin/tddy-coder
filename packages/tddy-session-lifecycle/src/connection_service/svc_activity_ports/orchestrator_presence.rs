use crate::connection_service::svc_activity_ports::ACTIVITY_SERVICE;
use crate::connection_service::svc_activity_ports::PRESENCE_RPC;
use crate::connection_service::DaemonSessionHost;
use async_trait::async_trait;
use prost::Message as _;
use tddy_daemon_kernel::peer_forwarding::PeerRoute;
use tddy_rpc::Status;
use tddy_service::proto::activity::{
    IsSessionTerminalServedRequest, IsSessionTerminalServedResponse,
};
use tddy_session_activity::orchestrator_presence::{OrchestratorPresence, PEER_PRESENCE_TIMEOUT};
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tddy_terminal_rpc::session::TerminalSessionStore as _;

/// An orchestrator is present when the daemon it runs on serves its main terminal — the served
/// terminal, not a pid, so a dead home whose pid was reused never makes a child wait. This daemon
/// answers for itself; for an orchestrator on another host it asks that host
/// ([`DaemonSessionHost::peer_serves_main_terminal`]), `#home-inbox` 10/11.
pub(super) struct ServedTerminalPresence {
    pub(super) terminals: crate::terminal_session_adapter::DaemonTerminalSessionStore,
    /// This daemon's roster and common room, which a peer is asked through.
    pub(super) connection: DaemonSessionHost,
}

#[async_trait]
impl OrchestratorPresence for ServedTerminalPresence {
    async fn is_active(
        &self,
        orchestrator_session_id: &str,
        orchestrator_daemon_instance_id: Option<&str>,
    ) -> bool {
        match orchestrator_daemon_instance_id {
            None => self
                .terminals
                .get_terminal(orchestrator_session_id, MAIN_TERMINAL_ID)
                .await
                .is_some(),
            Some(host) => {
                self.connection
                    .peer_serves_main_terminal(host, orchestrator_session_id)
                    .await
            }
        }
    }
}

impl DaemonSessionHost {
    /// Whether the daemon `peer_instance_id` names serves `session_id`'s main terminal. A daemon
    /// other than this one is asked with `ActivityService.IsSessionTerminalServed`, over the roster
    /// and unary forward [`PeerRoutedActivity`] uses, within [`PEER_PRESENCE_TIMEOUT`]. This daemon
    /// is answered from its own terminals.
    ///
    /// **Absent on any doubt**: an id that is not on the roster, no common room, a peer that errors,
    /// or one that does not answer in time all answer `false`, so a dead peer costs the child's
    /// Claude prompt at most the bound and never a wait.
    async fn peer_serves_main_terminal(&self, peer_instance_id: &str, session_id: &str) -> bool {
        match self.classify_addressed_daemon_route(PRESENCE_RPC, peer_instance_id) {
            Ok(PeerRoute::Forward {
                peer_instance_id: peer,
            }) => match self.ask_peer_whether_it_serves(&peer, session_id).await {
                Ok(served) => served,
                Err(status) => {
                    log::warn!(
                        "{PRESENCE_RPC}: {session_id} counted as absent: daemon_instance_id={peer} \
                         did not answer: {:?} {}",
                        status.code(),
                        status.message()
                    );
                    false
                }
            },
            // The host named is this one, which the gate folds into `None`; answered from this
            // daemon's own terminals all the same.
            Ok(PeerRoute::Local) => self
                .terminal_sessions()
                .get_terminal(session_id, MAIN_TERMINAL_ID)
                .await
                .is_some(),
            Err(status) => {
                log::warn!(
                    "{PRESENCE_RPC}: {session_id} counted as absent: daemon_instance_id={peer_instance_id} \
                     is not routable: {}",
                    status.message()
                );
                false
            }
        }
    }

    /// Ask `peer` whether it serves `session_id`'s main terminal, bounded by
    /// [`PEER_PRESENCE_TIMEOUT`]. Any failure is returned for the caller to count as absent.
    async fn ask_peer_whether_it_serves(
        &self,
        peer: &str,
        session_id: &str,
    ) -> Result<bool, Status> {
        let request = IsSessionTerminalServedRequest {
            session_id: session_id.to_string(),
        };
        let answered = self
            .common_room_slot(PRESENCE_RPC)?
            .forward_to_peer_within(
                peer,
                ACTIVITY_SERVICE,
                PRESENCE_RPC,
                request.encode_to_vec(),
                PEER_PRESENCE_TIMEOUT,
            )
            .await?;
        IsSessionTerminalServedResponse::decode(answered.as_slice())
            .map(|answer| answer.served)
            .map_err(|e| Status::internal(format!("decode {PRESENCE_RPC} answer from peer: {e}")))
    }
}
