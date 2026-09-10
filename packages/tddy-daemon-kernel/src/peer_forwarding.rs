//! Serving one daemon's RPC on another daemon, over the LiveKit common room.
//!
//! Every multi-host RPC in this system is addressed by a `daemon_instance_id`, and the daemon that
//! takes the call is very often not the one that can answer it: a host's tooling probe, the prompts
//! it raises and the keys it can be given are all facts about **one machine**, and answering them
//! locally would hand the caller this daemon's own answer wearing another host's name.
//!
//! This module is the two halves of that: [`classify_peer_route`] decides whether a call is ours,
//! and [`forward_to_peer`] / [`forward_server_stream_to_peer`] carry it to the daemon it belongs
//! to. They live in the kernel because the *subsystems* that route — hosts today, more later — are
//! leaving the daemon crate, while the common-room connection they route over stays in it.
//!
//! `livekit_peer_discovery` re-exports every name here, so no caller in the daemon changed.

use std::sync::Arc;
use std::time::Duration;

use livekit::Room;

/// Whether an RPC should run locally or be forwarded to a discovered peer.
///
/// Previously named `StartSessionPeerRoute`; renamed to `PeerRoute` to reflect that this
/// routing logic is now applied to all eligible RPCs (StartSession, ExecuteTool, ListExecTools, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerRoute {
    Local,
    Forward { peer_instance_id: String },
}

/// Decide routing from the requested `daemon_instance_id`, local id, and currently eligible peer ids.
///
/// Generic rename of `classify_start_session_peer_route`; the latter is now a thin wrapper.
pub fn classify_peer_route(
    local_instance_id: &str,
    requested_instance_id: &str,
    eligible_instance_ids: &[String],
) -> Result<PeerRoute, String> {
    let req = requested_instance_id.trim();
    if req.is_empty() {
        log::debug!(
            "classify_peer_route: empty requested id → Local (local_instance_id={})",
            local_instance_id
        );
        return Ok(PeerRoute::Local);
    }
    let local = local_instance_id.trim();
    if req == local {
        log::debug!("classify_peer_route: requested matches local → Local");
        return Ok(PeerRoute::Local);
    }
    if eligible_instance_ids.iter().any(|id| id.trim() == req) {
        log::info!(
            "classify_peer_route: forwarding RPC to peer instance_id={}",
            req
        );
        return Ok(PeerRoute::Forward {
            peer_instance_id: req.to_string(),
        });
    }
    Err(format!(
        "unknown or not connected daemon_instance_id {:?}: peer is not in the current eligible daemon list (configure livekit.common_room and ensure the peer is in the same LiveKit room)",
        req
    ))
}

/// The LiveKit identity a daemon **serves** its RPC services on: a fixed `daemon-` prefix over the
/// instance id, matching what `main.rs` joins the common room with.
///
/// A daemon is present in the common room twice: a *discovery* participant under the bare instance
/// id, which publishes the advertisement and runs no RPC server, and this *RPC* participant. The
/// eligible-daemon list is built from the discovery identity, so every routing decision yields a
/// bare id that has to be mapped here before a request is published — addressing the bare id
/// reaches a participant that never answers.
///
/// See `docs/ft/web/daemon-selector-livekit-rpc.md` § The daemon identity subtlety.
pub fn daemon_rpc_identity(instance_id: &str) -> String {
    // Composed from the constant `token.TokenService` refuses to mint, so no client can ever be
    // admitted under a name that lands here.
    format!(
        "{}{}",
        tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX,
        instance_id.trim()
    )
}

/// Deadline for one forwarded unary RPC, and for opening a forwarded server stream.
///
/// A LiveKit data-channel call has no transport-level timeout: if the peer's RPC participant is
/// gone (or never answers), the pending call is simply never resolved and the caller waits
/// forever — the operator sees an action that neither completes nor fails. Generous enough for a
/// forward carrying a whole attachment's bytes over chunked data frames, short enough that a dead
/// peer surfaces as an error rather than a hang.
pub const PEER_FORWARD_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a forwarded server stream may go without a frame before it is terminated as an error.
///
/// A stream that stops producing must never look like a stream that ended: the consumer would
/// write a truncated document as if it were whole (see `docs/ft/coder/rpc-multi-transport.md` —
/// a single lost chunk frame wedges a call with no error).
/// Also bounded from below, by `tddy_tools::session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE`: a
/// subscriber classifies a roster pass by how long it lasted, so tearing a forwarded stream down
/// faster than that threshold would make every relay-idle teardown read as churn and park a
/// cross-host roster at its reconnect ceiling. `tddy-tools` is only a dev-dependency here, so that
/// relation is pinned by a test rather than a `const` assert — see
/// `tests/session_agent_roster_acceptance.rs`.
pub const PEER_FORWARD_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

fn peer_forward_deadline_status(
    service: &str,
    method: &str,
    peer_id: &str,
    waited: Duration,
) -> tddy_rpc::Status {
    tddy_rpc::Status::deadline_exceeded(format!(
        "forwarding {service}/{method} to daemon {peer_id} timed out after {}s: the peer is in the common room but its RPC participant did not answer",
        waited.as_secs()
    ))
}

/// An RPC client addressed at a peer daemon's [`daemon_rpc_identity`], drawn from the common room's
/// shared client factory.
///
/// The factory keeps **one** request-id registry and **one** response loop per room connection,
/// regardless of how many peers are forwarded to or how often. Building a client per call (as
/// `forward_to_peer` once did) leaked a `subscribe()` loop each time.
async fn peer_client(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_id: &str,
    service: &str,
    method: &str,
    what: &str,
) -> Result<tddy_livekit::RpcClient, tddy_rpc::Status> {
    let room_arc = {
        let g = room_slot.read().await;
        g.clone()
    }
    .ok_or_else(|| {
        tddy_rpc::Status::failed_precondition(format!(
            "LiveKit common room is not connected on this daemon; cannot forward {what} to a peer"
        ))
    })?;
    let target = daemon_rpc_identity(peer_id);
    log::debug!(
        "peer_client: peer_id={} target_identity={} service={} method={} ({what})",
        peer_id,
        target,
        service,
        method
    );
    Ok(tddy_livekit::LiveKitRpcClientFactory::for_room(room_arc).client(target))
}

/// Forward a generic RPC to a peer daemon in the common room via LiveKit data-channel RPC.
///
/// This is the generic building block for all peer-forwarding. It:
/// 1. Draws a client for the peer's [`daemon_rpc_identity`] ([`peer_client`], which also returns
///    `failed_precondition` if no room is connected).
/// 2. Calls `{service}/{method}` with the given `body` bytes, bounded by [`PEER_FORWARD_TIMEOUT`].
pub async fn forward_to_peer(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_id: &str,
    service: &str,
    method: &str,
    body: Vec<u8>,
) -> Result<Vec<u8>, tddy_rpc::Status> {
    forward_to_peer_within(
        room_slot,
        peer_id,
        service,
        method,
        body,
        PEER_FORWARD_TIMEOUT,
    )
    .await
}

/// [`forward_to_peer`] with an explicit deadline, for the few calls whose peer-side work is bounded
/// by something other than a round trip.
///
/// [`PEER_FORWARD_TIMEOUT`] is sized for a peer that answers promptly or not at all. A call the peer
/// spends minutes serving — cloning a repository, cutting a worktree — needs a deadline drawn from
/// *that* budget instead, or the caller gives up while the peer is still building and both sides end
/// up believing something different about what exists.
pub async fn forward_to_peer_within(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_id: &str,
    service: &str,
    method: &str,
    body: Vec<u8>,
    deadline: Duration,
) -> Result<Vec<u8>, tddy_rpc::Status> {
    let client = peer_client(room_slot, peer_id, service, method, "an RPC").await?;
    tokio::time::timeout(deadline, client.call_unary(service, method, body))
        .await
        .map_err(|_| peer_forward_deadline_status(service, method, peer_id, deadline))?
}

/// Forward a **server-streaming** RPC to a peer daemon, relaying each frame decoded by
/// `decode_frame` into the returned receiver.
///
/// The transport already does the streaming ([`tddy_livekit::RpcClient::call_server_stream`]);
/// what a daemon needs on top is the peer's [`daemon_rpc_identity`], a deadline, and the guarantee
/// that a stream which stops without its end-of-stream marker terminates **as an error** rather
/// than as a short but successful stream. That distinction is the whole point: a caller writing
/// frames to disk cannot tell a truncated document from a complete one.
pub async fn forward_server_stream_to_peer<T, D>(
    room_slot: &Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    peer_id: &str,
    service: &str,
    method: &str,
    body: Vec<u8>,
    decode_frame: D,
) -> Result<tokio::sync::mpsc::UnboundedReceiver<Result<T, tddy_rpc::Status>>, tddy_rpc::Status>
where
    T: Send + 'static,
    D: Fn(Vec<u8>) -> Result<T, tddy_rpc::Status> + Send + 'static,
{
    let client = peer_client(room_slot, peer_id, service, method, "a stream").await?;
    let mut frames = tokio::time::timeout(
        PEER_FORWARD_TIMEOUT,
        client.call_server_stream(service, method, body),
    )
    .await
    .map_err(|_| peer_forward_deadline_status(service, method, peer_id, PEER_FORWARD_TIMEOUT))??;

    // Deliberately unbounded, even though the transport channel underneath it is bounded (32
    // frames): that channel is filled by the room's **shared** response loop with
    // `send().await` (`ClientEngine::on_response`), so a relay that stopped draining it would
    // block response dispatch for every other in-flight call on this daemon's common-room
    // connection — head-of-line blocking across unrelated RPCs, which is worse than buffering one
    // stream. What bounds the buffer instead is the payload: both callers cap what they accept
    // (`max_attachment_bytes` re-checked while accumulating in
    // `fetch_peer_staged_attachment`, the same cap checked before the first frame in
    // `stream_read_host_document`), and both relayed streams are short-lived. Bounding this
    // safely needs backpressure the transport can express without stalling its shared loop.
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let peer_id = peer_id.to_string();
    let service = service.to_string();
    let method = method.to_string();
    tokio::spawn(async move {
        // The client carries this call's registration in the room's shared engine; hold it for the
        // stream's whole life so the response loop keeps delivering frames to it.
        let _client = client;
        loop {
            let item =
                match tokio::time::timeout(PEER_FORWARD_STREAM_IDLE_TIMEOUT, frames.recv()).await {
                    Ok(Some(Ok(bytes))) => decode_frame(bytes),
                    // A mid-stream failure arrives as a single terminal error frame.
                    Ok(Some(Err(status))) => Err(status),
                    // The peer sent end-of-stream: the relayed stream ends the same way.
                    Ok(None) => break,
                    Err(_) => Err(peer_forward_deadline_status(
                        &service,
                        &method,
                        &peer_id,
                        PEER_FORWARD_STREAM_IDLE_TIMEOUT,
                    )),
                };
            let terminal = item.is_err();
            if tx.send(item).is_err() || terminal {
                break;
            }
        }
    });
    Ok(rx)
}
