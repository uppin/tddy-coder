use crate::service::activity_delta_frames;
use crate::service::ports::{DeltaLookup, DeltaScope};
use crate::service::relayed;
use crate::service::streamed;
use crate::service::ActivityServiceImpl;
use crate::session_notifications::{
    notification_for_activity_status, notification_for_agent_tool_call,
};
use crate::streams::{
    acp_replay_frame, relay_acp_replay, relay_acp_replay_count, relay_agent_activity,
    relay_session_notifications, seq_by_tool_call,
};
use async_trait::async_trait;
use std::path::PathBuf;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::activity::{
    AcpReplayFrame, AgentActivityDeltaChunk, AgentActivityDeltaRequest,
    AgentActivityRecord as ProtoAgentActivityRecord, DeltaScope as ProtoDeltaScope,
    GetAcpReplayPageRequest, GetAcpReplayPageResponse, GetAcpToolCallDetailRequest,
    GetAcpToolCallDetailResponse, ReportAgentActivityRequest, ReportAgentActivityResponse,
    ReportSessionStatusRequest, ReportSessionStatusResponse, SessionNotificationEvent,
    StreamAcpReplayRequest, StreamMode, StreamSessionActivityRequest,
    StreamSessionNotificationsRequest,
};
use tddy_worktree_service::stream::MpscResultStream;

#[async_trait]
impl tddy_service::proto::activity::ActivityService for ActivityServiceImpl {
    /// The per-worktree `claude` / `cursor` hook reporting what its session is doing.
    ///
    /// Everything is validated before any IO, and the credential before any write: an unknown
    /// status or a bad `hook_token` must not be able to touch a session directory at all.
    async fn report_session_status(
        &self,
        request: Request<ReportSessionStatusRequest>,
    ) -> Result<Response<ReportSessionStatusResponse>, Status> {
        let req = request.into_inner();

        // The status string is checked before the path is resolved: an unknown status is a bad
        // request whatever session it names.
        tddy_core::SessionActivityStatus::from_wire(&req.status)
            .ok_or_else(|| Status::invalid_argument(format!("unknown status: {}", req.status)))?;

        let (sessions_base, session_dir) = self.hook_session_dir(&req.os_user, &req.session_id)?;
        let meta = self.hook_session(&session_dir)?;

        // claude-cli and cursor-cli sessions support hook status reporting. Refused rather than
        // recorded for anything else: a status on a session type that never raises one would be a
        // row nothing keeps current — and refused *before* the credential, so a hook wired to the
        // wrong session is told that rather than that its token is bad.
        let session_type = meta.session_type.as_deref().unwrap_or("");
        if session_type != "claude-cli" && session_type != "cursor-cli" {
            return Err(Status::failed_precondition(
                "session_type is not claude-cli or cursor-cli",
            ));
        }

        // A local-process string comparison: the token is a per-session secret written into the
        // hook command at worktree-prep time, and both ends are on this host.
        if meta.hook_token.as_deref().unwrap_or("") != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        tddy_core::update_activity_status(&session_dir, &req.status)
            .map_err(|e| Status::internal(format!("failed to update activity status: {}", e)))?;

        log::debug!(
            target: "tddy_daemon::connection_service",
            "report_session_status: session={} status={}",
            req.session_id,
            req.status
        );

        // One publish, every interested subscriber: Telegram renders the attention-worthy ones,
        // and the notification stream carries all of them to the drawer's indicators. A subscriber
        // that fails is logged by the bus and never fails this hook (PRD NFR3).
        //
        // The notification names `req.os_user` as its owner — the same user whose sessions
        // directory the hook token was just checked against — so the stream can hand it to that
        // operator's clients and to no one else's.
        self.publish_for_session(&sessions_base, &req.session_id, |label| {
            notification_for_activity_status(
                &req.session_id,
                &req.os_user,
                label,
                &req.status,
                tddy_daemon_kernel::now_unix_ms(),
            )
        })
        .await;

        Ok(Response::new(ReportSessionStatusResponse { ok: true }))
    }

    /// The per-worktree hook reporting one of its agent's tool calls, before and after it runs.
    async fn report_agent_activity(
        &self,
        request: Request<ReportAgentActivityRequest>,
    ) -> Result<Response<ReportAgentActivityResponse>, Status> {
        let req = request.into_inner();

        let (sessions_base, session_dir) = self.hook_session_dir(&req.os_user, &req.session_id)?;
        let meta = self.hook_session(&session_dir)?;
        // Validated before anything is read off the checkout or appended to the log: the token is a
        // per-session secret, and this path writes.
        if meta.hook_token.as_deref().unwrap_or("") != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        // AC1/AC2 of `docs/ft/daemon/session-worktree-sync.md`: the record names the commit it was
        // made against and the paths it declared, so a consumer holding a patch can place it. The
        // HEAD is read from the filesystem rather than by spawning `git rev-parse` — an agent makes
        // a great many tool calls, and a subprocess on each would be paid on every one of them.
        //
        // A session with no checkout on this host stamps neither: `read_head_commit` returns an
        // empty string when HEAD cannot be resolved, and a path has nothing to be relative to. That
        // is the honest answer AC1 asks for, and the reason no sha is invented in its place.
        let worktree_root = meta.repo_path.as_deref().map(PathBuf::from);
        let head_commit = worktree_root
            .as_deref()
            .map(tddy_core::git_head::read_head_commit)
            .unwrap_or_default();
        let input = tddy_core::agent_activity::parse_activity_json(&req.input_json);
        let changed_paths = worktree_root
            .as_deref()
            .map(|root| tddy_core::agent_activity::declared_paths(&req.tool_name, &input, root))
            .unwrap_or_default();

        let record = match req.event.as_str() {
            "PreToolUse" => {
                // A tool call started: mint a call_id, remember it so the paired PostToolUse can
                // reuse it, and append the `running` row.
                let call_id = uuid::Uuid::new_v4().to_string();
                self.ports.activity.push_pending(&req.session_id, &call_id);
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: tddy_core::agent_activity::STATUS_RUNNING.to_string(),
                    result: serde_json::Value::Null,
                    error_message: String::new(),
                    started_unix_ms: tddy_daemon_kernel::now_unix_ms(),
                    completed_unix_ms: 0,
                    source: "claude-cli".to_string(),
                    head_commit,
                    // The tick that covers this call has not been measured yet; the poll loop
                    // attributes it when it runs. `NO_TICK` is the wire's "no tick has covered it
                    // yet", which a session's first real tick is distinguishable from — see
                    // [`crate::next_tick`].
                    activity_seq: crate::NO_TICK,
                    changed_paths,
                }
            }
            "PostToolUse" => {
                // The tool call finished: pair with the most-recent pending call_id (fresh id when
                // none is outstanding, e.g. a hook restart), and append the terminal row.
                let call_id = self
                    .ports
                    .activity
                    .pop_pending(&req.session_id)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let status = if req.is_error {
                    tddy_core::agent_activity::STATUS_ERROR
                } else {
                    tddy_core::agent_activity::STATUS_COMPLETED
                };
                tddy_core::agent_activity::AgentActivityRecord {
                    call_id,
                    tool_name: req.tool_name,
                    input,
                    status: status.to_string(),
                    result: tddy_core::agent_activity::parse_activity_json(&req.result_json),
                    error_message: req.error_message,
                    started_unix_ms: 0,
                    completed_unix_ms: tddy_daemon_kernel::now_unix_ms(),
                    source: "claude-cli".to_string(),
                    head_commit,
                    // As on the `running` row: the covering tick is the poll loop's to attribute.
                    activity_seq: crate::NO_TICK,
                    changed_paths,
                }
            }
            other => {
                return Err(Status::invalid_argument(format!(
                    "unknown event: {other} (expected PreToolUse or PostToolUse)"
                )));
            }
        };

        // The durable log is the source of truth; a write failure must not fail the hook call.
        if let Err(e) = tddy_core::agent_activity::append_agent_activity(&session_dir, &record) {
            log::warn!(
                target: "tddy_daemon::connection_service",
                "agent_activity: failed to persist {} for session {}: {}",
                req.event,
                req.session_id,
                e
            );
        }
        // The agent's own tool loop is the other thing that means "this session is working", and
        // the only one a cursor-cli or tool session reports at all. Owned by `req.os_user`, as at
        // the activity-status site above: the notification stream relays it to that operator only.
        let tool_name = record.tool_name.clone();
        self.publish_for_session(&sessions_base, &req.session_id, |label| {
            Some(notification_for_agent_tool_call(
                &req.session_id,
                &req.os_user,
                label,
                &tool_name,
                tddy_daemon_kernel::now_unix_ms(),
            ))
        })
        .await;
        self.ports.activity.publish(&req.session_id, record);
        // The record is **not** broadcast into the session room from here, deliberately.
        //
        // A record announced at this point names a tick nothing has measured yet: its
        // `activity_seq` is still `NO_TICK` and the delta covering its files is produced by the
        // next poll tick, so a participant that reacted to it and asked for the call's delta would
        // be told `UnknownCall` — an announcement that arrives before the thing it announces.
        //
        // The poll loop is the single broadcaster instead, tailing `agent-activity.jsonl`, which is
        // also what makes cursor-cli and tool sessions visible: their agents never call this RPC at
        // all, and a room fed only from here would carry claude-cli activity and nothing else.
        Ok(Response::new(ReportAgentActivityResponse { ok: true }))
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

        let detail = tddy_service::acp_replay::tool_call_detail(&session_dir, &req.tool_call_id)
            .map_err(|e| Status::internal(format!("read transcript: {e}")))?;
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
        let transcript = tddy_service::acp_replay::read_session_transcript(&session_dir)
            .map_err(|e| Status::internal(format!("read transcript: {e}")))?;
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
