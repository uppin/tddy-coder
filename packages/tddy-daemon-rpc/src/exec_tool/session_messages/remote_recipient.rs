//! Where a message's recipient runs, when it may be another connected host (n5, cross-host
//! messages).
//!
//! A recipient this host holds is messaged here, as ever. Otherwise its host is found from what the
//! sender's host already knows, without asking anyone — the caller's own orchestrator runs on the
//! host its changeset names, and the sender of a message the caller received runs on the host that
//! message was relayed from — so a message to either is queued even while the link is down. Any
//! other recipient is looked up: the named host, or every connected host, is asked for its own
//! sessions with the caller's token, and only a row a host reports for itself is believed. A host
//! the room does not list, or a named host that cannot be asked, is an explicit refusal — a message
//! is never queued for a host nobody can confirm.

use tddy_rpc::Status;
use tddy_service::proto::session::{ListSessionsRequest, ListSessionsResponse};
use tddy_session_activity::session_messages::InboxMessage;
use tddy_session_lifecycle::peer_routing::PeerRouting;
use tddy_tool_engine::session_message_tools::PostMessageRequest;
use tddy_worktree_service::session_target::HomeCaller;
use tokio::task::JoinSet;

use super::DaemonSessionMessages;

const SESSION_SERVICE: &str = "session.SessionService";

/// The longest a recipient is looked for on the other hosts: a host that has not answered by then
/// does not hold up the message, which is refused instead when it was the one named.
const LOCATE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// The other connected hosts, as a message's sender sees them.
pub(super) struct RecipientHosts {
    pub(super) routing: PeerRouting,
    /// This host, as the roster names it.
    pub(super) local_id: String,
    /// The caller's token: what another host is asked for its sessions with.
    pub(super) session_token: String,
}

/// Why a recipient's host was not found.
pub(super) enum Locating {
    /// No host the caller may reach holds the recipient in the caller's project: refused as not
    /// addressable, exactly as on one host.
    NotAddressable,
    /// The refusal, worded: a host the room does not list, or one that could not be asked.
    Refused(String),
}

impl DaemonSessionMessages {
    /// The host `request`'s recipient runs on: `None` for this host, `Some` for another.
    pub(super) async fn recipient_host(
        &self,
        caller: &HomeCaller,
        request: &PostMessageRequest,
        orchestrator: Option<&str>,
        orchestrator_host: Option<String>,
        received: Option<&InboxMessage>,
    ) -> Result<Option<String>, Locating> {
        let to = request.to_session_id.as_str();
        let named = request
            .daemon_instance_id
            .clone()
            .filter(|host| *host != self.peers.local_id);
        let known = |host: &Option<String>| named.is_none() || named.as_ref() == host.as_ref();
        // The caller's own orchestrator, on the host its changeset names.
        if orchestrator == Some(to) && orchestrator_host.is_some() && known(&orchestrator_host) {
            return Ok(orchestrator_host);
        }
        // The sender of the message replied to, on the host it was relayed from.
        if let Some(received) = received.filter(|received| received.from_session_id == to) {
            if received.from_daemon_instance_id.is_some()
                && known(&received.from_daemon_instance_id)
            {
                return Ok(received.from_daemon_instance_id.clone());
            }
        }
        if named.is_none() {
            let here = tddy_core::read_session_metadata(
                &tddy_core::session_lifecycle::unified_session_dir_path(&self.sessions_base, to),
            );
            if let Ok(metadata) = here {
                return if metadata.project_id == caller.project_id {
                    Ok(None)
                } else {
                    Err(Locating::NotAddressable)
                };
            }
            if request.daemon_instance_id.is_some() {
                // Named this host, which does not hold it.
                return Err(Locating::NotAddressable);
            }
        }
        let (host, project_id) = self.peers.locate(to, named.as_deref()).await?;
        if project_id != caller.project_id {
            return Err(Locating::NotAddressable);
        }
        Ok(Some(host))
    }
}

impl RecipientHosts {
    /// The host holding `session_id` and its project, asking `named` or every other connected
    /// host — all at once, taking the first host that reports it as its own.
    async fn locate(
        &self,
        session_id: &str,
        named: Option<&str>,
    ) -> Result<(String, String), Locating> {
        let eligible = self.routing.eligible_instance_ids();
        let hosts: Vec<String> = match named {
            Some(host) => {
                if !eligible.iter().any(|listed| listed == host) {
                    return Err(Locating::Refused(format!(
                        "host {host} is not a connected host: name a daemon_instance_id ListProjectSessions reports, or omit it to have the recipient found; the message was not sent"
                    )));
                }
                vec![host.to_string()]
            }
            None => eligible
                .into_iter()
                .filter(|host| *host != self.local_id)
                .collect(),
        };
        let mut asking = JoinSet::new();
        for host in hosts {
            let routing = self.routing.clone();
            let ask = ListSessionsRequest {
                session_token: self.session_token.clone(),
                include_peers: false,
            };
            asking.spawn(async move {
                let answer: Result<Option<ListSessionsResponse>, Status> = routing
                    .rpc_served_by_peer(SESSION_SERVICE, "ListSessions", &host, &ask)
                    .await;
                (host, answer)
            });
        }
        let mut unanswered = Vec::new();
        let until = tokio::time::Instant::now() + LOCATE_DEADLINE;
        loop {
            let joined = match tokio::time::timeout_at(until, asking.join_next()).await {
                Ok(Some(joined)) => joined,
                Ok(None) => break,
                Err(_) => {
                    unanswered.push(format!(
                        "no host answered within {} s",
                        LOCATE_DEADLINE.as_secs()
                    ));
                    break;
                }
            };
            let Ok((host, answer)) = joined else {
                continue;
            };
            match answer {
                Ok(Some(listed)) => {
                    if let Some(row) = listed
                        .sessions
                        .iter()
                        .find(|row| row.session_id == session_id && row.daemon_instance_id == host)
                    {
                        return Ok((host, row.project_id.clone()));
                    }
                }
                Ok(None) => {}
                Err(status) => unanswered.push(format!("{host}: {}", status.message())),
            }
        }
        match named {
            Some(host) if !unanswered.is_empty() => Err(Locating::Refused(format!(
                "host {host} could not be asked for session {session_id} ({}); the message was not sent",
                unanswered.join("; ")
            ))),
            _ => Err(Locating::NotAddressable),
        }
    }
}
