use crate::service::activity_delta_frames;
use crate::service::conversation_replay;
use crate::service::ports::{DeltaLookup, DeltaScope};
use crate::service::relayed;
use crate::service::streamed;
use crate::service::ActivityServiceImpl;
use crate::streams::{
    acp_replay_frame, relay_acp_replay, relay_acp_replay_count, relay_agent_activity,
    relay_session_notifications, seq_by_tool_call,
};
use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::activity::{
    AcpReplayFrame, AgentActivityDeltaChunk, AgentActivityDeltaRequest,
    AgentActivityRecord as ProtoAgentActivityRecord, AwaitQuestionAnswerRequest,
    AwaitQuestionAnswerResponse, DeltaScope as ProtoDeltaScope, GetAcpReplayPageRequest,
    GetAcpReplayPageResponse, GetAcpToolCallDetailRequest, GetAcpToolCallDetailResponse,
    IsSessionTerminalServedRequest, IsSessionTerminalServedResponse, ReportAgentActivityRequest,
    ReportAgentActivityResponse, ReportOperatorNotificationRequest,
    ReportOperatorNotificationResponse, ReportSessionStatusRequest, ReportSessionStatusResponse,
    SessionNotificationEvent, StreamAcpReplayRequest, StreamMode, StreamSessionActivityRequest,
    StreamSessionNotificationsRequest,
};
use tddy_worktree_service::stream::MpscResultStream;

#[async_trait]
impl tddy_service::proto::activity::ActivityService for ActivityServiceImpl {
    /// The per-worktree `claude` / `cursor` hook reporting what its session is doing — see
    /// `hook_reports`.
    async fn report_session_status(
        &self,
        request: Request<ReportSessionStatusRequest>,
    ) -> Result<Response<ReportSessionStatusResponse>, Status> {
        self.report_session_status_impl(request.into_inner()).await
    }

    /// The per-worktree hook reporting one of its agent's tool calls, before and after it runs —
    /// see `hook_reports`.
    async fn report_agent_activity(
        &self,
        request: Request<ReportAgentActivityRequest>,
    ) -> Result<Response<ReportAgentActivityResponse>, Status> {
        self.report_agent_activity_impl(request.into_inner()).await
    }

    /// An agent notifying its operator in its own words (`tddy-tools notify`) — see
    /// `operator_notifications`.
    async fn report_operator_notification(
        &self,
        request: Request<ReportOperatorNotificationRequest>,
    ) -> Result<Response<ReportOperatorNotificationResponse>, Status> {
        self.report_operator_notification_impl(request.into_inner())
            .await
    }

    /// An orchestrated session's `PermissionRequest` hook waiting for its orchestrator's answer —
    /// see `question_answers`.
    async fn await_question_answer(
        &self,
        request: Request<AwaitQuestionAnswerRequest>,
    ) -> Result<Response<AwaitQuestionAnswerResponse>, Status> {
        self.await_question_answer_impl(request.into_inner()).await
    }

    /// A peer asking whether this daemon serves an orchestrator's terminal — see
    /// `question_answers`.
    async fn is_session_terminal_served(
        &self,
        request: Request<IsSessionTerminalServedRequest>,
    ) -> Result<Response<IsSessionTerminalServedResponse>, Status> {
        self.is_session_terminal_served_impl(request.into_inner())
            .await
    }

    type StreamSessionActivityStream = MpscResultStream<ProtoAgentActivityRecord>;

    /// Stream a session's agent activity: replay the persisted `agent-activity.jsonl` snapshot,
    /// then relay live records published to the hub for this session.
    async fn stream_session_activity(
        &self,
        request: Request<StreamSessionActivityRequest>,
    ) -> Result<Response<Self::StreamSessionActivityStream>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ProtoAgentActivityRecord>();

        // Snapshot-then-live (the default and proto3 zero value) replays the coalesced on-disk
        // records first, then relays everything subsequently published to the hub for this
        // session. Live-only skips the snapshot entirely and carries only records published after
        // subscribe.
        let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);
        if mode == StreamMode::SnapshotThenLive {
            let snapshot =
                tddy_core::agent_activity::read_agent_activity(&session_dir).unwrap_or_default();
            for record in snapshot {
                if tx
                    .send(tddy_service::agent_activity_to_proto(record))
                    .is_err()
                {
                    // Receiver already gone — hand back a stream that terminates immediately.
                    return Ok(relayed(rx));
                }
            }
        }

        let broadcast_rx = self.ports.activity.subscribe(&req.session_id);
        tokio::spawn(relay_agent_activity(broadcast_rx, tx));

        Ok(relayed(rx))
    }

    type StreamSessionNotificationsStream = MpscResultStream<SessionNotificationEvent>;

    /// Stream every session notification this host raises, for as long as the client stays
    /// connected.
    ///
    /// Daemon-level by design (PRD NFR1): the request names no session, because one subscription
    /// serves a drawer of any size. It does *not* name a user either — the caller's own token
    /// does, and the relay carries only the sessions belonging to the OS user it maps to.
    /// Live-only: each event carries the moment it happened, and a replayed backlog would raise
    /// indicators for turns that finished while the tab was closed.
    async fn stream_session_notifications(
        &self,
        request: Request<StreamSessionNotificationsRequest>,
    ) -> Result<Response<Self::StreamSessionNotificationsStream>, Status> {
        let req = request.into_inner();

        // Authenticated and authorized exactly as `stream_session_activity` is: a token that maps
        // to no OS user owns no sessions on this host, so there is nothing it may be shown.
        let os_user = (self.ports.os_users)(&req.session_token)?;

        let broadcast_rx = self
            .ports
            .notifications
            .as_ref()
            .and_then(|bus| bus.subscribe_clients())
            .ok_or_else(|| {
                Status::failed_precondition("this daemon publishes no session notifications")
            })?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SessionNotificationEvent>();
        tokio::spawn(relay_session_notifications(broadcast_rx, tx, os_user));

        Ok(relayed(rx))
    }

    type StreamAgentActivityDeltaStream = MpscResultStream<AgentActivityDeltaChunk>;

    /// The tick delta lookup — AC6-AC14 of `docs/ft/daemon/session-worktree-sync.md`.
    ///
    /// The delta lives in the session room's ring, which is why this is answered through
    /// [`SessionDeltaStores`](crate::service::ports::SessionDeltaStores) rather than from disk: a patch is a measurement of a live checkout,
    /// and the host hosting that room is the only one that took it.
    ///
    /// Authorization comes **first**, before the ring is even looked up, for the reason AC14
    /// gives: an unauthenticated caller must not be able to learn which sessions this host holds
    /// by reading apart a `NOT_FOUND` from a `PERMISSION_DENIED`.
    async fn stream_agent_activity_delta(
        &self,
        request: Request<AgentActivityDeltaRequest>,
    ) -> Result<Response<Self::StreamAgentActivityDeltaStream>, Status> {
        let req = request.into_inner();
        (self.ports.os_users)(&req.session_token)?;

        let call_id = req.call_id.trim();
        if call_id.is_empty() {
            return Err(Status::invalid_argument(
                "call_id is required; there is no whole-worktree delta",
            ));
        }

        let scope = match ProtoDeltaScope::try_from(req.scope).unwrap_or(ProtoDeltaScope::Call) {
            ProtoDeltaScope::Call => DeltaScope::Call,
            ProtoDeltaScope::Residual => DeltaScope::Residual,
            ProtoDeltaScope::Tick => DeltaScope::Tick,
        };

        // Each absence carries its own message, because the client's response to each differs: the
        // wrong host is a routing mistake, an unknown call is a defect to report, and an aged-out
        // delta is an ordinary reconcile from the WIP ref. One shared message would make a long
        // mirror's routine recovery indistinguishable from a bug on one side or the other.
        let delta = match self
            .ports
            .deltas
            .delta_for_call(&req.session_id, call_id, scope)?
        {
            DeltaLookup::Found(delta) => delta,
            DeltaLookup::NoRoomHere => {
                return Err(Status::not_found(format!(
                    "no session room is hosted here for session {}, so it has no deltas",
                    req.session_id
                )))
            }
            DeltaLookup::UnknownCall => {
                return Err(Status::not_found(format!(
                    "unknown call {call_id}: this daemon has no record of it in session {}",
                    req.session_id
                )))
            }
            DeltaLookup::AgedOut { seq } => {
                return Err(Status::not_found(format!(
                    "delta for call {call_id} (tick {seq}) has aged out of this session's ring; reconcile from the WIP ref"
                )))
            }
        };

        Ok(streamed(activity_delta_frames(&delta)))
    }

    type StreamAcpReplayStream = MpscResultStream<AcpReplayFrame>;

    /// Stream a session's read-only ACP transcript: replay the session's resolved transcript
    /// snapshot (`acp-transcript.jsonl` merged with the durable `agent-activity.jsonl` — see
    /// [`tddy_service::acp_replay::read_session_transcript`]), then relay live agent-activity
    /// records (mapped to ACP `tool_call` frames) published to the hub for this session. Mirrors
    /// `stream_session_activity` — same auth and [`StreamMode`] semantics.
    async fn stream_acp_replay(
        &self,
        request: Request<StreamAcpReplayRequest>,
    ) -> Result<Response<Self::StreamAcpReplayStream>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;

        // A subagent conversation replays its own recorded transcript, then tails the frames the conversation
        // records while the stream is open. Its frames are published to their own channel, not the session hub.
        if !req.conversation_id.is_empty() {
            let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);
            return conversation_replay::stream_conversation(
                &self.ports.conversation_frames,
                &session_dir,
                &req.session_id,
                &req.conversation_id,
                mode,
                req.page_size,
            );
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<AcpReplayFrame>();

        // Snapshot-then-live (the default and proto3 zero value) replays the persisted transcript
        // first, then relays everything subsequently published to the hub for this session.
        // Live-only skips the snapshot entirely and carries only frames produced after subscribe.
        let mode = StreamMode::try_from(req.mode).unwrap_or(StreamMode::SnapshotThenLive);

        // Count-first mode emits only the running count of persisted transcript frames — one frame
        // now with the current count, then a fresh count each time a record is published — with no
        // transcript payload. It never replays the snapshot itself.
        if mode == StreamMode::CountThenLive {
            let snapshot =
                tddy_service::acp_replay::read_session_transcript(&session_dir).unwrap_or_default();
            let count = tddy_service::acp_replay::count_activity_entries(&snapshot);
            let seen_ids = tddy_service::acp_replay::tool_call_ids(&snapshot);
            if tx
                .send(AcpReplayFrame {
                    acp_agent_message: Vec::new(),
                    activity_count: count,
                    // A count frame carries no transcript payload, so it has no position.
                    seq: 0,
                })
                .is_err()
            {
                // Receiver already gone — hand back a stream that terminates immediately.
                return Ok(relayed(rx));
            }
            let broadcast_rx = self.ports.activity.subscribe(&req.session_id);
            tokio::spawn(relay_acp_replay_count(broadcast_rx, tx, count, seen_ids));
            return Ok(relayed(rx));
        }

        // The resolved transcript is what every position refers to: the replayed frames index into
        // it, and the live tail continues its numbering from the end of it. Live-only replays none
        // of it but still needs its length, so a live frame's `seq` means the same thing there.
        let snapshot =
            tddy_service::acp_replay::read_session_transcript(&session_dir).unwrap_or_default();

        // Which slice of the transcript is replayed on subscribe, and where in the transcript that
        // slice starts: all of it (snapshot-then-live, the proto3 default), its newest page only
        // (tail-then-live), or none of it (live-only).
        let (first_seq, replayed): (u64, &[tddy_service::proto::acp::AcpAgentMessage]) = match mode
        {
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
                .send(acp_replay_frame(frame, first_seq + offset as u64))
                .is_err()
            {
                // Receiver already gone — hand back a stream that terminates immediately.
                return Ok(relayed(rx));
            }
        }

        let broadcast_rx = self.ports.activity.subscribe(&req.session_id);
        tokio::spawn(relay_acp_replay(
            broadcast_rx,
            tx,
            snapshot.len() as u64,
            seq_by_tool_call(&snapshot),
        ));

        Ok(relayed(rx))
    }

    /// Return one tool call's full `raw_input`/`raw_output` from the session's coalesced transcript
    /// (the bodies `stream_acp_replay` strips out). Mirrors `stream_acp_replay`'s auth and maps an
    /// unknown `tool_call_id` to `NOT_FOUND`.
    async fn get_acp_tool_call_detail(
        &self,
        request: Request<GetAcpToolCallDetailRequest>,
    ) -> Result<Response<GetAcpToolCallDetailResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;

        let detail = if req.conversation_id.is_empty() {
            tddy_service::acp_replay::tool_call_detail(&session_dir, &req.tool_call_id)
                .map_err(|e| Status::internal(format!("read transcript: {e}")))?
        } else {
            let transcript = conversation_replay::recorded_transcript(
                &session_dir,
                &req.session_id,
                &req.conversation_id,
            )?;
            tddy_service::acp_replay::tool_call_detail_in(&transcript, &req.tool_call_id)
        };
        match detail {
            None => Err(Status::not_found(format!(
                "no tool call with id {} in session {}",
                req.tool_call_id, req.session_id
            ))),
            Some(detail) => Ok(Response::new(GetAcpToolCallDetailResponse {
                raw_input: detail.raw_input,
                raw_output: detail.raw_output,
            })),
        }
    }

    /// Return one page of transcript frames strictly older than `before_seq` — the reverse cursor a
    /// tail-first replay pages backwards with. Applies the same `strip_tool_body` seam the replay
    /// stream does: a paged frame is not a back door to the bodies.
    async fn get_acp_replay_page(
        &self,
        request: Request<GetAcpReplayPageRequest>,
    ) -> Result<Response<GetAcpReplayPageResponse>, Status> {
        let req = request.into_inner();
        let session_dir = self.session_dir(&req.session_token, &req.session_id)?;

        // A transcript that cannot be read is an error, never an empty page: an empty page means
        // "you have reached the head", and a reader told that stops paging for good.
        let transcript = if req.conversation_id.is_empty() {
            tddy_service::acp_replay::read_session_transcript(&session_dir)
                .map_err(|e| Status::internal(format!("read transcript: {e}")))?
        } else {
            conversation_replay::recorded_transcript(
                &session_dir,
                &req.session_id,
                &req.conversation_id,
            )?
        };
        let page = tddy_service::acp_replay::page_before(
            &transcript,
            req.before_seq,
            usize::try_from(req.page_size).unwrap_or(usize::MAX),
        );

        Ok(Response::new(GetAcpReplayPageResponse {
            frames: page
                .frames
                .iter()
                .map(|frame| {
                    prost::Message::encode_to_vec(&tddy_service::acp_replay::strip_tool_body(frame))
                })
                .collect(),
            first_seq: page.first_seq,
            at_oldest: page.at_oldest,
        }))
    }
}
