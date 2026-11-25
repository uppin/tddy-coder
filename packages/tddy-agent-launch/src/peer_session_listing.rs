//! `ListSessions` across every connected daemon: what `include_peers` adds to this host's own list.
//!
//! Every other daemon on the eligible-daemon roster is asked at once, over the peer forward
//! ([`PeerRouting::rpc_served_by_peer`]), with the caller's own token and `include_peers: false` —
//! so a listing never travels more than one hop. Only the rows a peer reports for **itself**
//! (`daemon_instance_id` naming that peer) are kept: a peer speaks for its own sessions, never for
//! another host's. A peer that does not answer is named with its reason, never silently dropped.
//!
//! Non-blocking: each peer is asked on its own task, so the listing waits for the slowest peer and
//! not for the sum of them — and never longer than [`PEER_LISTING_DEADLINE`], far below the
//! forward's own deadline, because the session drawer polls this listing every two seconds. A peer
//! that has not answered by then is named unreachable, saying so; it is listed again on the next
//! poll that it answers in time.

use tddy_host_service::peer_routing::PeerRouting;
use tddy_rpc::Status;
use tddy_service::proto::session::{
    ListSessionsRequest, ListSessionsResponse, SessionEntry, UnreachableSessionHost,
};
use tokio::task::JoinHandle;

use crate::session_coordinate_handlers::SESSION_SERVICE;

/// The longest the listing waits for the peers' answers, all of them together.
pub(crate) const PEER_LISTING_DEADLINE: std::time::Duration = std::time::Duration::from_secs(3);

/// One peer's `ListSessions`, asked and not yet answered.
type PendingAnswer = JoinHandle<Result<Option<ListSessionsResponse>, Status>>;

/// Every other connected daemon, asked for its own sessions.
pub(crate) struct PeerSessionListing {
    asking: Vec<(String, PendingAnswer)>,
}

/// What the other daemons answered: their own sessions, and the daemons that gave none.
#[derive(Default)]
pub(crate) struct PeerSessions {
    pub(crate) sessions: Vec<SessionEntry>,
    pub(crate) unreachable_hosts: Vec<UnreachableSessionHost>,
}

impl PeerSessionListing {
    /// Ask every eligible daemon but `local_id`, at once, for the sessions `session_token`'s owner
    /// has there. The asks run while the caller lists this host's own.
    pub(crate) fn ask(routing: &PeerRouting, local_id: &str, session_token: &str) -> Self {
        let ask = ListSessionsRequest {
            session_token: session_token.to_string(),
            include_peers: false,
        };
        let asking = routing
            .eligible_instance_ids()
            .into_iter()
            .filter(|peer| peer != local_id)
            .map(|peer| {
                let routing = routing.clone();
                let ask = ask.clone();
                let asked = peer.clone();
                let pending = tokio::spawn(async move {
                    routing
                        .rpc_served_by_peer::<_, ListSessionsResponse>(
                            SESSION_SERVICE,
                            "ListSessions",
                            &asked,
                            &ask,
                        )
                        .await
                });
                (peer, pending)
            })
            .collect();
        Self { asking }
    }

    /// Every answer, in roster order, waiting at most [`PEER_LISTING_DEADLINE`] in all.
    pub(crate) async fn answers(self) -> PeerSessions {
        self.answers_within(PEER_LISTING_DEADLINE).await
    }

    /// Every answer, in roster order, waiting at most `deadline` in all.
    pub(crate) async fn answers_within(self, deadline: std::time::Duration) -> PeerSessions {
        let until = tokio::time::Instant::now() + deadline;
        let mut gathered = PeerSessions::default();
        for (peer, pending) in self.asking {
            let abort = pending.abort_handle();
            let answer = match tokio::time::timeout_at(until, pending).await {
                Ok(joined) => joined
                    .unwrap_or_else(|e| Err(Status::internal(format!("asking it failed: {e}")))),
                Err(_) => {
                    abort.abort();
                    Err(Status::deadline_exceeded(format!(
                        "it did not answer within {} s",
                        deadline.as_secs_f32()
                    )))
                }
            };
            match answer {
                Ok(Some(listed)) => {
                    gathered
                        .sessions
                        .extend(own_sessions_of(&peer, listed.sessions));
                }
                // The roster names this daemon under another spelling: its sessions are the
                // local ones, already listed.
                Ok(None) => {}
                Err(status) => {
                    log::info!(
                        "ListSessions: daemon {peer} did not answer: {}",
                        status.message()
                    );
                    gathered.unreachable_hosts.push(UnreachableSessionHost {
                        daemon_instance_id: peer,
                        reason: status.message().to_string(),
                    });
                }
            }
        }
        gathered
    }
}

/// The rows `peer` lists for itself; a row naming any other daemon is not `peer`'s to report.
fn own_sessions_of(peer: &str, listed: Vec<SessionEntry>) -> impl Iterator<Item = SessionEntry> {
    let peer = peer.to_string();
    listed.into_iter().filter(move |s| {
        let own = s.daemon_instance_id == peer;
        if !own {
            log::debug!(
                "ListSessions: daemon {peer} listed session {} as on daemon {:?}; dropped",
                s.session_id,
                s.daemon_instance_id
            );
        }
        own
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_row(session_id: &str, host: &str) -> SessionEntry {
        SessionEntry {
            session_id: session_id.to_string(),
            daemon_instance_id: host.to_string(),
            ..SessionEntry::default()
        }
    }

    #[tokio::test]
    async fn a_peer_that_does_not_answer_in_time_is_named_and_does_not_hold_the_listing() {
        let silent: PendingAnswer = tokio::spawn(std::future::pending());
        let listing = PeerSessionListing {
            asking: vec![("host-c".to_string(), silent)],
        };

        let started = std::time::Instant::now();
        let answered = listing
            .answers_within(std::time::Duration::from_millis(50))
            .await;

        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(
            answered
                .unreachable_hosts
                .iter()
                .map(|host| host.daemon_instance_id.as_str())
                .collect::<Vec<_>>(),
            vec!["host-c"]
        );
    }

    #[test]
    fn a_peer_reports_only_its_own_sessions() {
        let listed = vec![
            a_row("own", "host-b"),
            a_row("claimed-elsewhere", "host-c"),
            a_row("unnamed", ""),
        ];

        let kept: Vec<String> = own_sessions_of("host-b", listed)
            .map(|s| s.session_id)
            .collect();

        assert_eq!(kept, vec!["own".to_string()]);
    }
}
