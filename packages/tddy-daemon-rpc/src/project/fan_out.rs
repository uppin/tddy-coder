//! The logical-project fan-out `UpdateProject`, `DeleteProject` and `RestoreProject` share: the
//! receiving daemon applies the change to its own row, then forwards the same request — marked
//! `local_only` — to every eligible peer at once. A peer that does not own the project answers
//! `not_found` and is skipped; one that cannot be reached, or answers with anything else, is
//! *unconfirmed*: whether it holds the project is unknown, and it converges the next time a listing
//! reaches it (see [`super::lifecycle_listing`]).

use std::future::Future;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_rpc::{Code, Status};
use tddy_service::proto::project::UnconfirmedHost;

use super::ProjectRpcHandler;

/// What the peers answered one fanned-out leg.
pub(super) struct PeerAnswers<R> {
    /// Peers that applied it, with their answers, ordered by peer id.
    pub applied: Vec<(String, R)>,
    /// Peers that failed with anything but `not_found`, ordered by peer id.
    pub failed: Vec<(String, Status)>,
}

impl<R> PeerAnswers<R> {
    pub fn unconfirmed(&self) -> Vec<UnconfirmedHost> {
        self.failed
            .iter()
            .map(|(peer, status)| UnconfirmedHost {
                daemon_instance_id: peer.clone(),
                reason: status.message.clone(),
            })
            .collect()
    }
}

impl ProjectRpcHandler {
    /// Forward `request` through `forward` to every eligible peer but this daemon, concurrently.
    /// Each forward is bounded by the room's own forward timeout. With no common-room connection
    /// every peer is unconfirmed, saying so.
    pub(super) async fn fan_out_to_peers<Req, R, F, Fut>(
        &self,
        rpc: &'static str,
        request: Req,
        forward: F,
    ) -> PeerAnswers<R>
    where
        Req: Clone + Send + 'static,
        R: Send + 'static,
        F: Fn(CommonRoom, String, Req) -> Fut,
        Fut: Future<Output = Result<R, Status>> + Send + 'static,
    {
        let local_id = local_instance_id_for_config(&self.config);
        let peers: Vec<String> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|d| d.instance_id.0)
            .filter(|id| *id != local_id)
            .collect();
        let mut answers = PeerAnswers {
            applied: Vec::new(),
            failed: Vec::new(),
        };
        let Some(room) = self.common_room_livekit_room.clone() else {
            answers.failed = peers
                .into_iter()
                .map(|peer| {
                    let status = Status::unavailable(format!(
                        "cannot forward {rpc}: this process has no common-room connection"
                    ));
                    (peer, status)
                })
                .collect();
            return answers;
        };
        let mut legs = tokio::task::JoinSet::new();
        for peer in peers {
            let leg = forward(room.clone(), peer.clone(), request.clone());
            legs.spawn(async move { (peer, leg.await) });
        }
        while let Some(joined) = legs.join_next().await {
            let Ok((peer, answer)) = joined else {
                continue;
            };
            match answer {
                Ok(reply) => answers.applied.push((peer, reply)),
                Err(status) if status.code == Code::NotFound => {}
                Err(status) => {
                    log::info!("{rpc}: peer {peer} unconfirmed: {}", status.message);
                    answers.failed.push((peer, status));
                }
            }
        }
        answers.applied.sort_by(|a, b| a.0.cmp(&b.0));
        answers.failed.sort_by(|a, b| a.0.cmp(&b.0));
        answers
    }
}
