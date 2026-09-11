//! The four live relays `activity.ActivityService`'s streaming methods hand back, and the frame
//! shapes they produce.
//!
//! Moved out of `tddy-daemon`'s `connection_service` by `#unbundle` node 7 with the handlers that
//! spawn them. Each one reads a [`tddy_daemon_kernel::AgentActivityHub`] or a
//! [`crate::session_notifications::SessionNotificationBus`] receiver and writes frames into an
//! mpsc channel until the client hangs up — so the relay, not the handler, is what owns a
//! subscription's whole life, and all four lose the daemon nothing by living here: none of them
//! touches a peer, a room or a config.

use std::collections::{HashMap, HashSet};

use prost::Message as _;
use tddy_core::agent_activity::AgentActivityRecord;
use tddy_service::proto::acp::AcpAgentMessage;
use tddy_service::proto::activity::{
    AcpReplayFrame, AgentActivityRecord as ProtoAgentActivityRecord, SessionNotificationEvent,
    SessionNotificationKind as ProtoSessionNotificationKind,
    SessionNotificationSource as ProtoSessionNotificationSource,
};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};

/// One session notification on the wire.
///
/// [`SessionNotification::os_user`] is dropped here rather than carried: it is what decides
/// *whether* a client is shown the event at all (see [`relay_session_notifications`]), and the
/// drawer has no use for it. Putting an authorization fact on the wire would only tell a browser
/// something about the host's other operators that it has no reason to know.
#[must_use]
pub fn session_notification_event(notification: SessionNotification) -> SessionNotificationEvent {
    SessionNotificationEvent {
        session_id: notification.session_id,
        label: notification.label,
        kind: match notification.kind {
            SessionNotificationKind::Activity => ProtoSessionNotificationKind::Activity,
            SessionNotificationKind::AttentionRequired => {
                ProtoSessionNotificationKind::AttentionRequired
            }
        } as i32,
        source: match notification.source {
            SessionNotificationSource::ActivityStatus => {
                ProtoSessionNotificationSource::ActivityStatus
            }
            SessionNotificationSource::AgentToolCall => {
                ProtoSessionNotificationSource::AgentToolCall
            }
            SessionNotificationSource::Presenter => ProtoSessionNotificationSource::Presenter,
        } as i32,
        text: notification.text,
        at_unix_ms: notification.at_unix_ms,
    }
}

/// Relay task for `StreamSessionNotifications`: forwards `os_user`'s session notifications to one
/// client until it disconnects. A client that falls behind the channel's capacity loses its oldest
/// events (`Lagged`) and keeps its stream: the newest notification is the one an indicator is
/// derived from, so dropping the stream over a stale one would cost more than the gap.
///
/// The bus is daemon-wide — one channel carries every session on the host, which is what lets a
/// drawer of any size pay for a single subscription (PRD NFR1). Scoping to one operator is
/// therefore this relay's job: without it, a daemon serving several users would hand each of them
/// the others' session ids, repository names and operator-facing text.
pub async fn relay_session_notifications(
    mut broadcast_rx: Receiver<SessionNotification>,
    tx: UnboundedSender<SessionNotificationEvent>,
    os_user: String,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(notification) => {
                // Delivered only on a positive match of a named owner. A notification that names
                // no owner is not a notification for everybody — it is one whose owner could not
                // be established, and the safe answer to that is to deliver it to no one.
                if notification.os_user.is_empty() || notification.os_user != os_user {
                    continue;
                }
                if tx.send(session_notification_event(notification)).is_err() {
                    break;
                }
            }
            Err(RecvError::Lagged(missed)) => {
                log::debug!(
                    target: "tddy_daemon::session_notifications",
                    "a notification stream client fell behind and missed {missed} event(s)"
                );
            }
            Err(RecvError::Closed) => break,
        }
    }
}

/// Relay task for `StreamSessionActivity`: forwards live agent-activity records for one session
/// (the broadcast is already session-scoped) from the hub into `tx` until the client disconnects.
pub async fn relay_agent_activity(
    mut broadcast_rx: Receiver<AgentActivityRecord>,
    tx: UnboundedSender<ProtoAgentActivityRecord>,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                if tx
                    .send(tddy_service::agent_activity_to_activity_proto(record))
                    .is_err()
                {
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// Wrap one ACP frame in the [`AcpReplayFrame`] envelope, encoding the inner `AcpAgentMessage` to
/// its protobuf bytes and stamping its absolute transcript position.
///
/// `seq` is the frame's 0-based index in the session's *resolved* transcript
/// ([`tddy_service::acp_replay::read_session_transcript`]) — the same list
/// [`tddy_service::acp_replay::page_before`] indexes, so the reverse cursor a client reads off a
/// frame addresses the same position the pager does.
#[must_use]
pub fn acp_replay_frame(frame: &AcpAgentMessage, seq: u64) -> AcpReplayFrame {
    AcpReplayFrame {
        acp_agent_message: tddy_service::acp_replay::strip_tool_body(frame).encode_to_vec(),
        // A transcript frame carries no count; the count-first mode sets this instead.
        activity_count: 0,
        seq,
    }
}

/// The absolute 0-based position of every tool call in a resolved transcript, keyed by
/// `tool_call_id`.
///
/// Seeds [`relay_acp_replay`]'s live numbering: when a call whose `running` record is already in
/// the snapshot reports its terminal record, that record refines the snapshot entry and so must
/// carry the snapshot's position for it — not a fresh position at the tail.
#[must_use]
pub fn seq_by_tool_call(frames: &[AcpAgentMessage]) -> HashMap<String, u64> {
    frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| {
            tddy_service::acp_replay::tool_call_id_of(frame)
                .map(|id| (id.to_string(), index as u64))
        })
        .collect()
}

/// Relay task for `StreamAcpReplay`: forwards live agent-activity records for one session (the
/// broadcast is already session-scoped) as enriched ACP `tool_call` replay frames into `tx` until
/// the client disconnects.
///
/// `next_seq` is the resolved transcript's length at subscribe time, so the live tail continues the
/// snapshot's numbering and a frame delivered live carries the position a later re-read would give
/// it.
///
/// A tool call broadcasts twice — its `running` record then its terminal one — but the two coalesce
/// into a *single* resolved transcript entry, so the refinement must land on the position its first
/// record was given instead of consuming one of its own. `seq_by_tool_call` remembers that mapping
/// and is pre-seeded from the snapshot ([`seq_by_tool_call`]), so a call straddling the subscribe
/// boundary refines the entry the snapshot already placed.
pub async fn relay_acp_replay(
    mut broadcast_rx: Receiver<AgentActivityRecord>,
    tx: UnboundedSender<AcpReplayFrame>,
    mut next_seq: u64,
    mut seq_by_tool_call: HashMap<String, u64>,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                let frame = tddy_service::acp_replay::frame_for_agent_activity(&record);
                let seq = match tddy_service::acp_replay::tool_call_id_of(&frame) {
                    Some(id) => *seq_by_tool_call.entry(id.to_string()).or_insert_with(|| {
                        let seq = next_seq;
                        next_seq += 1;
                        seq
                    }),
                    None => {
                        let seq = next_seq;
                        next_seq += 1;
                        seq
                    }
                };
                if tx.send(acp_replay_frame(&frame, seq)).is_err() {
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// Count-only relay task for `StreamAcpReplay`'s `CountThenLive` mode: each **newly-seen** tool
/// call published to the session hub bumps `count` by one and emits a fresh count-only
/// [`AcpReplayFrame`] (no transcript payload) into `tx`, until the client disconnects. A call's
/// `running` and terminal records share a `call_id` and so count once (matching the coalesced rows
/// the pane renders); `seen_ids` is pre-seeded with the snapshot's ids so a call straddling the
/// subscribe boundary is not double-counted. This is the cheap feed that drives the overlay's
/// activity badge before the full pane is opened.
pub async fn relay_acp_replay_count(
    mut broadcast_rx: Receiver<AgentActivityRecord>,
    tx: UnboundedSender<AcpReplayFrame>,
    mut count: u64,
    mut seen_ids: HashSet<String>,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                if !seen_ids.insert(record.call_id) {
                    // A record for a call already counted (its terminal row, or a snapshot
                    // straddler).
                    continue;
                }
                count += 1;
                if tx
                    .send(AcpReplayFrame {
                        acp_agent_message: Vec::new(),
                        activity_count: count,
                        // A count frame carries no transcript payload, so it has no position.
                        seq: 0,
                    })
                    .is_err()
                {
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}
