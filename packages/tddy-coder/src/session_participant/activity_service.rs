//! The coder's `activity.ActivityService` coordinate — families M and N as this process serves
//! them.
//!
//! The coder is the *second* server of the activity family, exactly as
//! [`super::terminal_session_service`] made it the second server of the terminal family: a session
//! reached over LiveKit is answered here and the same session reached over HTTP is answered by the
//! daemon. `#unbundle` node 7 moved these four off `connection.ConnectionService`, so this module
//! is where they answer.
//!
//! # Four of the eight
//!
//! `activity.ActivityService` declares eight methods. This participant serves the four that read
//! *its own* session: the activity stream, the ACP replay stream, one tool call's bodies and one
//! page of the transcript. The other four — `ReportSessionStatus`, `ReportAgentActivity`,
//! `StreamSessionNotifications` and `StreamAgentActivityDelta` — are the daemon's: the first two
//! are written by per-worktree hooks against the daemon's own stores, the third is a
//! daemon-wide feed across every session, and the fourth serves a worktree this process does not
//! hold. A participant answering any of them would be answering about state it does not have, so
//! they are refused here rather than faked, which is the shape
//! [`super::terminal_session_service`] already established for its own two.
//!
//! The handlers are the coder's own rather than `tddy-session-activity`'s, unlike the terminal
//! family: this process has no session token to authenticate, no session store to resolve a
//! directory from and no `AgentActivityHub` — its transcript is the one directory it is running
//! against, and its live tail is its own presenter broadcast.

use std::sync::Arc;

use async_trait::async_trait;
use prost::Message;

use tddy_rpc::{RpcMessage, RpcResult, RpcService, ServiceEntry, Status};
use tddy_service::proto::activity::{
    AcpReplayFrame, GetAcpReplayPageRequest, GetAcpReplayPageResponse, GetAcpToolCallDetailRequest,
    GetAcpToolCallDetailResponse, StreamAcpReplayRequest, StreamMode, StreamSessionActivityRequest,
};

use super::acp_transcript;
use super::connection_service_participant::SessionConnectionService;
use super::AGENT_ACTIVITY_CHANNEL_CAPACITY;

/// The `activity.ActivityService` entry the coder's participant registers.
///
/// Built from the same [`SessionConnectionService`] the `connection.ConnectionService` entry is, so
/// the replay this coordinate serves and the tool calls that coordinate reports come from one
/// transcript directory and one presenter broadcast.
#[must_use]
pub fn coder_activity_entry(svc: Arc<SessionConnectionService>) -> ServiceEntry {
    ServiceEntry {
        name: tddy_service::session_activity::ACTIVITY_SERVICE,
        service: Arc::new(CoderActivityRpc { svc }) as Arc<dyn RpcService>,
    }
}

/// Families M and N at the coder's participant, over any `tddy-rpc` transport.
struct CoderActivityRpc {
    svc: Arc<SessionConnectionService>,
}

#[async_trait]
impl RpcService for CoderActivityRpc {
    async fn handle_rpc(&self, _service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        match method {
            "StreamSessionActivity" => {
                let req = match StreamSessionActivityRequest::decode(&message.payload[..]) {
                    Ok(req) => req,
                    Err(e) => {
                        return RpcResult::ServerStream(Err(Status::invalid_argument(format!(
                            "decode StreamSessionActivityRequest: {e}"
                        ))));
                    }
                };
                let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);

                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Status>>(
                    AGENT_ACTIVITY_CHANNEL_CAPACITY,
                );

                // Subscribe to the live tail BEFORE snapshotting the durable log so a record
                // appended between the snapshot read and the first bridge recv() is still delivered
                // (via the broadcast) rather than dropped in the gap.
                let live_rx = self.svc.presenter_events.as_ref().map(|tx| tx.subscribe());

                // Snapshot-then-live (the default) replays the coalesced on-disk records first;
                // live-only skips the snapshot and carries only records that arrive after subscribe.
                if mode == StreamMode::SnapshotThenLive {
                    let snapshot = tddy_core::agent_activity::read_agent_activity(
                        &self.svc.agent_activity_dir,
                    )
                    .unwrap_or_default();
                    for record in snapshot {
                        let frame = tddy_service::agent_activity_to_proto(record).encode_to_vec();
                        if tx.try_send(Ok(frame)).is_err() {
                            // Receiver already gone — return the (now-closed) stream.
                            return RpcResult::ServerStream(Ok(rx));
                        }
                    }
                }

                // Live tail: forward every AgentActivity the presenter broadcasts, ending when the
                // presenter channel closes or the client disconnects.
                if let Some(mut live_rx) = live_rx {
                    tokio::spawn(async move {
                        use tokio::sync::broadcast::error::RecvError;
                        loop {
                            match live_rx.recv().await {
                                Ok(tddy_core::PresenterEvent::AgentActivity(record)) => {
                                    let frame = tddy_service::agent_activity_to_proto(record)
                                        .encode_to_vec();
                                    if tx.send(Ok(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(_) => continue,
                                Err(RecvError::Closed) => break,
                                Err(RecvError::Lagged(_)) => continue,
                            }
                        }
                    });
                }

                RpcResult::ServerStream(Ok(rx))
            }
            "StreamAcpReplay" => {
                let req = match StreamAcpReplayRequest::decode(&message.payload[..]) {
                    Ok(req) => req,
                    Err(e) => {
                        return RpcResult::ServerStream(Err(Status::invalid_argument(format!(
                            "decode StreamAcpReplayRequest: {e}"
                        ))));
                    }
                };
                let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);

                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Status>>(
                    AGENT_ACTIVITY_CHANNEL_CAPACITY,
                );

                // Wrap one ACP frame in the connection-local `AcpReplayFrame` envelope and encode it
                // to the transport bytes the client decodes back into an `AcpReplayFrame`. `seq` is
                // the frame's 0-based position in the session's resolved transcript — the same list
                // `page_before` indexes, so a cursor read off a frame addresses the same position
                // the pager does.
                fn replay_frame_bytes(
                    frame: &tddy_service::proto::acp::AcpAgentMessage,
                    seq: u64,
                ) -> Vec<u8> {
                    AcpReplayFrame {
                        acp_agent_message: tddy_service::acp_replay::strip_tool_body(frame)
                            .encode_to_vec(),
                        // A transcript frame carries no count; count-first mode sets this instead.
                        activity_count: 0,
                        seq,
                    }
                    .encode_to_vec()
                }

                // Encode a count-only `AcpReplayFrame` envelope (no transcript payload) carrying the
                // running number of persisted activity frames — the cheap feed for the overlay badge.
                fn count_frame_bytes(activity_count: u64) -> Vec<u8> {
                    AcpReplayFrame {
                        acp_agent_message: Vec::new(),
                        activity_count,
                        // A count frame carries no transcript payload, so it has no position.
                        seq: 0,
                    }
                    .encode_to_vec()
                }

                // Subscribe to the live tail BEFORE snapshotting so an event produced between the
                // snapshot read and the first bridge recv() is still delivered (via the broadcast)
                // rather than dropped in the gap.
                let live_rx = self.svc.presenter_events.as_ref().map(|tx| tx.subscribe());

                // Count-first mode emits only the running count of persisted transcript frames — one
                // frame now with the current count, then a fresh count for each subsequent renderable
                // presenter event — with no transcript payload. It never replays the snapshot itself.
                if mode == StreamMode::CountThenLive {
                    let snapshot = tddy_service::acp_replay::read_session_transcript(
                        &self.svc.agent_activity_dir,
                    )
                    .unwrap_or_default();
                    let mut count = tddy_service::acp_replay::count_activity_entries(&snapshot);
                    let mut seen_ids = tddy_service::acp_replay::tool_call_ids(&snapshot);
                    if tx.try_send(Ok(count_frame_bytes(count))).is_err() {
                        // Receiver already gone — return the (now-closed) stream.
                        return RpcResult::ServerStream(Ok(rx));
                    }
                    if let Some(mut live_rx) = live_rx {
                        tokio::spawn(async move {
                            use tddy_core::PresenterEvent;
                            use tokio::sync::broadcast::error::RecvError;
                            loop {
                                match live_rx.recv().await {
                                    Ok(event) => {
                                        // Count each new entry the pane would render: agent text
                                        // always, a tool call once (coalesced by call_id across its
                                        // running + terminal records).
                                        let counts = match &event {
                                            PresenterEvent::AgentOutput(_) => true,
                                            PresenterEvent::AgentActivity(record) => {
                                                seen_ids.insert(record.call_id.clone())
                                            }
                                            _ => false,
                                        };
                                        if !counts {
                                            continue;
                                        }
                                        count += 1;
                                        if tx.send(Ok(count_frame_bytes(count))).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(RecvError::Closed) => break,
                                    Err(RecvError::Lagged(_)) => continue,
                                }
                            }
                        });
                    }
                    return RpcResult::ServerStream(Ok(rx));
                }

                // The resolved transcript (the persisted ACP frames merged with the durable
                // agent-activity rows) is what every position refers to: the replayed frames index
                // into it, and the live tail continues its numbering from the end of it. Live-only
                // replays none of it but still needs its length, so a live frame's `seq` means the
                // same thing there.
                let snapshot =
                    tddy_service::acp_replay::read_session_transcript(&self.svc.agent_activity_dir)
                        .unwrap_or_default();

                // Which slice is replayed on subscribe, and where in the transcript it starts: all
                // of it (snapshot-then-live, the default), its newest page only (tail-then-live), or
                // none of it (live-only).
                let (first_seq, replayed): (u64, &[tddy_service::proto::acp::AcpAgentMessage]) =
                    match mode {
                        StreamMode::SnapshotThenLive => (0, &snapshot),
                        StreamMode::TailThenLive => {
                            let page = tddy_service::acp_replay::tail_page(
                                &snapshot,
                                usize::try_from(req.page_size).unwrap_or(usize::MAX),
                            );
                            (page.first_seq, page.frames)
                        }
                        _ => (0, &[]),
                    };
                for (offset, frame) in replayed.iter().enumerate() {
                    if tx
                        .try_send(Ok(replay_frame_bytes(frame, first_seq + offset as u64)))
                        .is_err()
                    {
                        // Receiver already gone — return the (now-closed) stream.
                        return RpcResult::ServerStream(Ok(rx));
                    }
                }

                // Live tail: map every renderable presenter event to its ACP frame (via the same
                // mapper the on-disk writer uses) and forward it, ending when the presenter channel
                // closes or the client disconnects. Numbering continues from the transcript's length
                // so a live frame carries the position a later re-read would give it.
                //
                // A tool call emits twice — its `running` event then its terminal one — but the two
                // coalesce into a *single* resolved transcript entry, so the refinement lands on the
                // position its first event was given rather than consuming one of its own. The map
                // is seeded from the snapshot so a call straddling the subscribe boundary refines
                // the entry the snapshot already placed. Mirrors the daemon's own replay host.
                if let Some(mut live_rx) = live_rx {
                    let mut next_seq = snapshot.len() as u64;
                    let mut seq_by_tool_call: std::collections::HashMap<String, u64> = snapshot
                        .iter()
                        .enumerate()
                        .filter_map(|(index, frame)| {
                            tddy_service::acp_replay::tool_call_id_of(frame)
                                .map(|id| (id.to_string(), index as u64))
                        })
                        .collect();
                    tokio::spawn(async move {
                        use tokio::sync::broadcast::error::RecvError;
                        loop {
                            match live_rx.recv().await {
                                Ok(event) => {
                                    let Some(frame) = acp_transcript::frame_for_event(
                                        &event,
                                        acp_transcript::now_unix_ms(),
                                    ) else {
                                        continue;
                                    };
                                    let seq =
                                        match tddy_service::acp_replay::tool_call_id_of(&frame) {
                                            Some(id) => *seq_by_tool_call
                                                .entry(id.to_string())
                                                .or_insert_with(|| {
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
                                    let bytes = replay_frame_bytes(&frame, seq);
                                    if tx.send(Ok(bytes)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(RecvError::Closed) => break,
                                Err(RecvError::Lagged(_)) => continue,
                            }
                        }
                    });
                }

                RpcResult::ServerStream(Ok(rx))
            }
            "GetAcpToolCallDetail" => {
                let req = match GetAcpToolCallDetailRequest::decode(&message.payload[..]) {
                    Ok(req) => req,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode GetAcpToolCallDetailRequest: {e}"
                        ))));
                    }
                };
                match tddy_service::acp_replay::tool_call_detail(
                    &self.svc.agent_activity_dir,
                    &req.tool_call_id,
                ) {
                    Err(e) => {
                        RpcResult::Unary(Err(Status::internal(format!("read transcript: {e}"))))
                    }
                    Ok(None) => RpcResult::Unary(Err(Status::not_found(format!(
                        "no tool call with id {} in this session",
                        req.tool_call_id
                    )))),
                    Ok(Some(d)) => RpcResult::Unary(Ok(GetAcpToolCallDetailResponse {
                        raw_input: d.raw_input,
                        raw_output: d.raw_output,
                    }
                    .encode_to_vec())),
                }
            }
            "GetAcpReplayPage" => {
                let req = match GetAcpReplayPageRequest::decode(&message.payload[..]) {
                    Ok(req) => req,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::invalid_argument(format!(
                            "decode GetAcpReplayPageRequest: {e}"
                        ))));
                    }
                };
                // A transcript that cannot be read is an error, never an empty page: an empty page
                // means "you have reached the head", and a reader told that stops paging for good.
                let transcript = match tddy_service::acp_replay::read_session_transcript(
                    &self.svc.agent_activity_dir,
                ) {
                    Ok(transcript) => transcript,
                    Err(e) => {
                        return RpcResult::Unary(Err(Status::internal(format!(
                            "read transcript: {e}"
                        ))));
                    }
                };
                let page = tddy_service::acp_replay::page_before(
                    &transcript,
                    req.before_seq,
                    usize::try_from(req.page_size).unwrap_or(usize::MAX),
                );
                // The same `strip_tool_body` seam the replay stream applies — a paged frame is not a
                // back door to the bodies.
                RpcResult::Unary(Ok(GetAcpReplayPageResponse {
                    frames: page
                        .frames
                        .iter()
                        .map(|frame| {
                            tddy_service::acp_replay::strip_tool_body(frame).encode_to_vec()
                        })
                        .collect(),
                    first_seq: page.first_seq,
                    at_oldest: page.at_oldest,
                }
                .encode_to_vec()))
            }
            other => RpcResult::Unary(Err(Status::unimplemented(format!(
                "the session participant does not serve ActivityService/{other}"
            )))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use tokio::sync::broadcast;

    use tddy_core::PresenterEvent;
    use tddy_service::session_activity::ACTIVITY_SERVICE;

    use crate::session_participant::connection_service_participant::{ToolExecutor, ToolOutcome};
    use crate::session_participant::terminal_manager;

    use tddy_core::agent_activity::{
        append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING,
    };
    use tddy_service::proto::activity::AgentActivityRecord as ProtoAgentActivityRecord;

    /// Executor that is never invoked by the `StreamSessionActivity` path.
    struct UnusedExecutor;
    #[async_trait]
    impl ToolExecutor for UnusedExecutor {
        async fn execute(&self, _tool_name: &str, _args_json: &str) -> ToolOutcome {
            ToolOutcome::default()
        }
    }

    fn a_running_record(call_id: &str) -> AgentActivityRecord {
        AgentActivityRecord {
            call_id: call_id.to_string(),
            tool_name: "Bash".to_string(),
            input: serde_json::json!({ "command": "cargo build" }),
            status: STATUS_RUNNING.to_string(),
            result: serde_json::Value::Null,
            error_message: String::new(),
            started_unix_ms: 1_700_000_000_000,
            completed_unix_ms: 0,
            source: "coder".to_string(),
            head_commit: String::new(),
            activity_seq: 0,
            changed_paths: Vec::new(),
        }
    }

    fn a_completed_record(call_id: &str) -> AgentActivityRecord {
        AgentActivityRecord {
            status: STATUS_COMPLETED.to_string(),
            result: serde_json::json!({ "stdout": "done" }),
            completed_unix_ms: 1_700_000_000_500,
            ..a_running_record(call_id)
        }
    }

    fn stream_request_message(session_id: &str) -> RpcMessage {
        let req = StreamSessionActivityRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    fn rpc_for(
        dir: &std::path::Path,
        events: broadcast::Sender<PresenterEvent>,
    ) -> CoderActivityRpc {
        CoderActivityRpc {
            svc: Arc::new(SessionConnectionService {
                session_id: "sess-1".to_string(),
                session_token: "session-token".to_string(),
                tool_calls_path: dir.join("tool-calls.jsonl"),
                tools: Vec::new(),
                executor: Arc::new(UnusedExecutor),
                worktree: dir.to_path_buf(),
                terminal_manager: Arc::new(terminal_manager::TerminalManager::new()),
                agent_activity_dir: dir.to_path_buf(),
                presenter_events: Some(events),
            }),
        }
    }

    async fn recv_record(
        rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
    ) -> ProtoAgentActivityRecord {
        let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("expected a streamed agent-activity frame")
            .expect("stream ended unexpectedly")
            .expect("frame carried an error status");
        ProtoAgentActivityRecord::decode(&frame[..]).expect("decode AgentActivityRecord")
    }

    #[tokio::test]
    async fn stream_session_activity_replays_the_persisted_snapshot_then_the_live_broadcast() {
        // Given — a session dir with one persisted (coalesced) call, and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-1")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamSessionActivity arm is dispatched
        let result = rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "StreamSessionActivity",
                &stream_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the snapshot's coalesced completed call arrives first
        let snapshot = recv_record(&mut rx).await;
        assert_eq!(snapshot.call_id, "call-1");
        assert_eq!(snapshot.status, STATUS_COMPLETED);
        assert_eq!(
            snapshot.result,
            tddy_service::json_to_proto_value(&serde_json::json!({ "stdout": "done" }))
        );

        // And — a subsequently-broadcast AgentActivity is forwarded live
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-2")))
            .expect("broadcast send");
        let live = recv_record(&mut rx).await;
        assert_eq!(live.call_id, "call-2");
        assert_eq!(live.status, STATUS_RUNNING);
        assert_eq!(live.tool_name, "Bash");
    }

    fn stream_request_message_live_only(session_id: &str) -> RpcMessage {
        let req = StreamSessionActivityRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: tddy_service::proto::activity::StreamMode::LiveOnly as i32,
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    #[tokio::test]
    async fn stream_session_activity_in_live_only_mode_skips_the_persisted_snapshot() {
        // Given — a session dir with a persisted call, and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-snapshot")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamSessionActivity arm is dispatched in LIVE_ONLY mode
        let result = rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "StreamSessionActivity",
                &stream_request_message_live_only("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // and — a record is broadcast live after the subscription
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-live")))
            .expect("broadcast send");

        // Then — the first frame is the live record; the persisted 'call-snapshot' was skipped
        let first = recv_record(&mut rx).await;
        assert_eq!(
            first.call_id, "call-live",
            "live-only must not replay the persisted snapshot ('call-snapshot')"
        );
    }

    fn acp_replay_request_message(session_id: &str) -> RpcMessage {
        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::SnapshotThenLive as i32,
            page_size: 0,
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    /// Receive one streamed replay byte-frame and decode its inner ACP `AcpAgentMessage`.
    async fn recv_acp_frame(
        rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
    ) -> tddy_service::proto::acp::AcpAgentMessage {
        let bytes = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("expected a streamed replay frame")
            .expect("stream ended unexpectedly")
            .expect("frame carried an error status");
        let envelope = AcpReplayFrame::decode(&bytes[..]).expect("decode AcpReplayFrame");
        tddy_service::proto::acp::AcpAgentMessage::decode(&envelope.acp_agent_message[..])
            .expect("decode inner AcpAgentMessage")
    }

    /// The text of an `agent_message_chunk` ACP frame (panics on any other shape).
    fn acp_agent_text(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
        use tddy_service::proto::acp::{acp_agent_message, content_block, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.as_ref()) {
                    Some(session_update::Update::AgentMessageChunk(c)) => {
                        match c.content.as_ref().and_then(|b| b.block.as_ref()) {
                            Some(content_block::Block::Text(t)) => t.text.clone(),
                            other => panic!("expected text content, got {other:?}"),
                        }
                    }
                    other => panic!("expected AgentMessageChunk, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    /// The tool_call_id of a `tool_call` ACP frame (panics on any other shape).
    fn acp_tool_call_id(frame: &tddy_service::proto::acp::AcpAgentMessage) -> String {
        use tddy_service::proto::acp::{acp_agent_message, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => {
                        tc.tool_call_id.map(|id| id.value).unwrap_or_default()
                    }
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stream_acp_replay_replays_the_persisted_transcript_then_the_live_broadcast() {
        // Given — a session dir with one persisted ACP transcript frame, and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing the parser.", 1_000),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamAcpReplay arm is dispatched
        let result = rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "StreamAcpReplay",
                &acp_replay_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the persisted agent-text frame arrives first
        let snapshot = recv_acp_frame(&mut rx).await;
        assert_eq!(acp_agent_text(&snapshot), "Analyzing the parser.");

        // And — a subsequently-broadcast AgentActivity is mapped to a live tool_call frame
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-2")))
            .expect("broadcast send");
        let live = recv_acp_frame(&mut rx).await;
        assert_eq!(acp_tool_call_id(&live), "call-2");
    }

    /// Receive one streamed byte-frame and decode the raw `AcpReplayFrame` envelope (the count
    /// carrier), with a timeout so a count-mode subscription that never emits fails fast.
    async fn recv_acp_envelope(
        rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
    ) -> AcpReplayFrame {
        let bytes = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("expected a streamed replay frame")
            .expect("stream ended unexpectedly")
            .expect("frame carried an error status");
        AcpReplayFrame::decode(&bytes[..]).expect("decode AcpReplayFrame")
    }

    #[tokio::test]
    async fn stream_acp_replay_count_then_live_broadcasts_the_activity_count() {
        // Given — a session dir with two persisted transcript frames and a presenter broadcast
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
        )
        .unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_running_record("call-a")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        // When — the StreamAcpReplay arm is dispatched in count-first mode
        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: "sess-1".to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
            page_size: 0,
        };
        let message = RpcMessage::new(req.encode_to_vec(), Default::default());
        let result = rpc
            .handle_rpc(ACTIVITY_SERVICE, "StreamAcpReplay", &message)
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the first frame carries the current count (2) and no transcript payload
        let first = recv_acp_envelope(&mut rx).await;
        assert_eq!(first.activity_count, 2);
        assert!(
            first.acp_agent_message.is_empty(),
            "a count frame must not carry a transcript payload"
        );

        // And — a subsequently-broadcast AgentActivity raises the count to 3
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-live")))
            .expect("broadcast send");
        let next = recv_acp_envelope(&mut rx).await;
        assert_eq!(next.activity_count, 3);
    }

    #[tokio::test]
    async fn stream_acp_replay_count_then_live_counts_a_tool_call_once_across_its_two_records() {
        // Given — a session dir with one persisted agent-text frame (count baseline 1)
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing.", 1_000),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());

        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: "sess-1".to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
            page_size: 0,
        };
        let message = RpcMessage::new(req.encode_to_vec(), Default::default());
        let mut rx = match rpc
            .handle_rpc(ACTIVITY_SERVICE, "StreamAcpReplay", &message)
            .await
        {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 1);

        // When — a tool call broadcasts its running then terminal record under one call_id
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-x")))
            .expect("broadcast send");
        // Then — the first (running) record lifts the count to 2
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 2);
        // The terminal record for call-x emits nothing; a distinct call is the next frame and reads
        // 3, proving call-x was counted once.
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-x")))
            .expect("broadcast send");
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-y")))
            .expect("broadcast send");
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 3);
    }

    // -----------------------------------------------------------------------
    // Persisted-activity replay (bug fc990524: badge counts, pane opens empty)
    //
    // `acp-transcript.jsonl` only exists for sessions that ran the presenter seam that writes it; a
    // session started before it (or one whose tool calls were recorded by another host) has only the
    // durable `agent-activity.jsonl`. Replaying the transcript file alone serves an empty snapshot
    // while the count feed keeps counting live records — badge, but nothing to see.
    // -----------------------------------------------------------------------

    /// The snapshot must project the session's durable `agent-activity.jsonl` rows, coalesced by
    /// call_id, when no ACP transcript was written.
    #[tokio::test]
    async fn stream_acp_replay_replays_persisted_agent_activity_when_no_acp_transcript_exists() {
        // Given — a session dir whose activity log holds one coalesced call and one still-running
        // call, and no `acp-transcript.jsonl`
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-2")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the StreamAcpReplay arm is dispatched in snapshot mode
        let result = rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "StreamAcpReplay",
                &acp_replay_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — both persisted calls are replayed as tool_call frames, each once, in recorded order
        assert_eq!(acp_tool_call_id(&recv_acp_frame(&mut rx).await), "call-1");
        assert_eq!(acp_tool_call_id(&recv_acp_frame(&mut rx).await), "call-2");
    }

    /// The count baseline comes from the same resolved transcript the snapshot replays, so a badge
    /// never promises entries the pane cannot deliver.
    #[tokio::test]
    async fn stream_acp_replay_count_then_live_counts_persisted_agent_activity_rows() {
        // Given — the same dir: two distinct calls persisted in the activity log alone
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_completed_record("call-1")).unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-2")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the StreamAcpReplay arm is dispatched in count-first mode
        let req = StreamAcpReplayRequest {
            session_token: "caller-token".to_string(),
            session_id: "sess-1".to_string(),
            daemon_instance_id: String::new(),
            mode: StreamMode::CountThenLive as i32,
            page_size: 0,
        };
        let message = RpcMessage::new(req.encode_to_vec(), Default::default());
        let mut rx = match rpc
            .handle_rpc(ACTIVITY_SERVICE, "StreamAcpReplay", &message)
            .await
        {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the first count frame reports the two persisted calls
        assert_eq!(recv_acp_envelope(&mut rx).await.activity_count, 2);
    }

    /// The full `ToolCall` payload of a `tool_call` ACP frame (panics on any other shape).
    fn acp_tool_call(
        frame: &tddy_service::proto::acp::AcpAgentMessage,
    ) -> tddy_service::proto::acp::ToolCall {
        use tddy_service::proto::acp::{acp_agent_message, session_update};
        match &frame.msg {
            Some(acp_agent_message::Msg::SessionUpdate(n)) => {
                match n.update.as_ref().and_then(|u| u.update.clone()) {
                    Some(session_update::Update::ToolCall(tc)) => tc,
                    other => panic!("expected ToolCall, got {other:?}"),
                }
            }
            other => panic!("expected a SessionUpdate frame, got {other:?}"),
        }
    }

    fn detail_request_message(session_id: &str, tool_call_id: &str) -> RpcMessage {
        let req = GetAcpToolCallDetailRequest {
            session_token: "caller-token".to_string(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            tool_call_id: tool_call_id.to_string(),
        };
        RpcMessage::new(req.encode_to_vec(), Default::default())
    }

    /// A `SNAPSHOT_THEN_LIVE` tool-call frame carries the call's id but not its bodies: the heavy
    /// `raw_input`/`raw_output` are stripped so the stream stays small.
    #[tokio::test]
    async fn stream_acp_replay_snapshot_frames_omit_tool_bodies() {
        // Given — a session dir whose persisted transcript holds a completed call with full bodies
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_completed_record("call-1")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the StreamAcpReplay snapshot arm is dispatched
        let result = rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "StreamAcpReplay",
                &acp_replay_request_message("sess-1"),
            )
            .await;
        let mut rx = match result {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        };

        // Then — the tool call arrives with its id intact but neither body
        let tc = acp_tool_call(&recv_acp_frame(&mut rx).await);
        assert_eq!(tc.tool_call_id.expect("tool_call_id").value, "call-1");
        assert_eq!(tc.raw_input, None);
        assert_eq!(tc.raw_output, None);
    }

    /// The bodies the stream strips are fetched on demand: GetAcpToolCallDetail returns the exact
    /// raw_input/raw_output the transcript recorded for one call.
    #[tokio::test]
    async fn get_acp_tool_call_detail_returns_the_full_tool_bodies() {
        // Given — a session dir whose transcript holds a completed Bash call
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_completed_record("call-1")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the detail for that call is requested
        let result = rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "GetAcpToolCallDetail",
                &detail_request_message("sess-1", "call-1"),
            )
            .await;
        let bytes = match result {
            RpcResult::Unary(Ok(bytes)) => bytes,
            RpcResult::Unary(Err(status)) => {
                panic!("expected a detail response, got error status: {status:?}")
            }
            _ => panic!("expected a unary result, got a server stream"),
        };
        let resp = GetAcpToolCallDetailResponse::decode(&bytes[..])
            .expect("decode GetAcpToolCallDetailResponse");

        // Then — it returns the exact bodies the stream used to inline
        let raw_input: serde_json::Value =
            serde_json::from_str(&resp.raw_input.expect("raw_input")).expect("raw_input is JSON");
        let raw_output: serde_json::Value =
            serde_json::from_str(&resp.raw_output.expect("raw_output"))
                .expect("raw_output is JSON");
        assert_eq!(raw_input, serde_json::json!({ "command": "cargo build" }));
        assert_eq!(raw_output, serde_json::json!({ "stdout": "done" }));
    }

    /// A tool_call_id absent from the transcript is a NOT_FOUND error, not an empty success.
    #[tokio::test]
    async fn get_acp_tool_call_detail_is_not_found_for_an_unknown_tool_call_id() {
        // Given — a session dir whose transcript holds only call-1
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::frame_for_agent_activity(&a_completed_record("call-1")),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events);

        // When — the detail for a non-existent call is requested
        let result = rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "GetAcpToolCallDetail",
                &detail_request_message("sess-1", "does-not-exist"),
            )
            .await;
        let status = match result {
            RpcResult::Unary(Err(status)) => status,
            RpcResult::Unary(Ok(_)) => panic!("expected NOT_FOUND, got a success response"),
            _ => panic!("expected a unary result, got a server stream"),
        };

        // Then — the status is NOT_FOUND
        assert_eq!(status.code(), tddy_rpc::Code::NotFound);
    }

    /// Dispatch the `StreamAcpReplay` arm and hand back its byte-frame stream, failing loudly on
    /// anything that is not a server stream.
    async fn acp_replay_stream(
        rpc: &CoderActivityRpc,
        session_id: &str,
    ) -> tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>> {
        match rpc
            .handle_rpc(
                ACTIVITY_SERVICE,
                "StreamAcpReplay",
                &acp_replay_request_message(session_id),
            )
            .await
        {
            RpcResult::ServerStream(Ok(rx)) => rx,
            RpcResult::ServerStream(Err(status)) => {
                panic!("expected a server stream, got error status: {status:?}")
            }
            _ => panic!("expected a server stream, got a unary result"),
        }
    }

    /// A tool call's terminal event refines the entry its `running` event created, so it carries
    /// that entry's position — the two coalesce into one transcript row, and this host must number
    /// them exactly as the daemon's own replay host does.
    #[tokio::test]
    async fn stream_acp_replay_gives_a_tool_calls_terminal_event_the_position_of_its_running_event()
    {
        // Given — a session dir with one persisted transcript frame, and a subscribed stream
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing the parser.", 1_000),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());
        let mut rx = acp_replay_stream(&rpc, "sess-1").await;
        assert_eq!(recv_acp_envelope(&mut rx).await.seq, 0);

        // When — one call broadcasts its running event and then its terminal one
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-1")))
            .expect("broadcast send");
        events
            .send(PresenterEvent::AgentActivity(a_completed_record("call-1")))
            .expect("broadcast send");

        // Then — both land on position 1, the single row they coalesce into
        let running = recv_acp_envelope(&mut rx).await;
        let terminal = recv_acp_envelope(&mut rx).await;
        assert_eq!((running.seq, terminal.seq), (1, 1));
    }

    /// A refinement costs the transcript no position: the next distinct call takes the very next
    /// one, so the live numbering neither drifts ahead of nor lags behind a later re-read.
    #[tokio::test]
    async fn stream_acp_replay_gives_the_call_after_a_refinement_the_next_position() {
        // Given — a session dir with one persisted transcript frame, and a subscribed stream
        let dir = tempfile::tempdir().unwrap();
        tddy_service::acp_replay::append_acp_frame(
            dir.path(),
            &tddy_service::acp_replay::agent_text_frame("Analyzing the parser.", 1_000),
        )
        .unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());
        let mut rx = acp_replay_stream(&rpc, "sess-1").await;
        assert_eq!(recv_acp_envelope(&mut rx).await.seq, 0);

        // When — one call runs to completion and a second, distinct call then starts
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-1")))
            .expect("broadcast send");
        events
            .send(PresenterEvent::AgentActivity(a_completed_record("call-1")))
            .expect("broadcast send");
        events
            .send(PresenterEvent::AgentActivity(a_running_record("call-2")))
            .expect("broadcast send");

        // Then — the new call is at 2: the refinement of call-1 consumed no position of its own
        recv_acp_envelope(&mut rx).await;
        recv_acp_envelope(&mut rx).await;
        assert_eq!(recv_acp_envelope(&mut rx).await.seq, 2);
    }

    /// A call that straddles the subscribe boundary — already recorded as running in the snapshot,
    /// finishing live — refines the snapshot's row, so it carries the position the snapshot gave
    /// that row rather than a fresh one at the tail.
    #[tokio::test]
    async fn stream_acp_replay_gives_a_live_terminal_event_the_position_its_call_holds_in_the_snapshot(
    ) {
        // Given — a session dir whose durable activity log holds a still-running call
        let dir = tempfile::tempdir().unwrap();
        append_agent_activity(dir.path(), &a_running_record("call-1")).unwrap();
        let (events, _keepalive) = broadcast::channel(16);
        let rpc = rpc_for(dir.path(), events.clone());
        let mut rx = acp_replay_stream(&rpc, "sess-1").await;
        assert_eq!(recv_acp_envelope(&mut rx).await.seq, 0);

        // When — that same call broadcasts its terminal event
        events
            .send(PresenterEvent::AgentActivity(a_completed_record("call-1")))
            .expect("broadcast send");

        // Then — it arrives at position 0, replacing the snapshot's row rather than following it
        assert_eq!(recv_acp_envelope(&mut rx).await.seq, 0);
    }
}
