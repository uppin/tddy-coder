# The agent-activity log, the hub, and the ACP replay stream

Moved here from `packages/tddy-daemon/docs/connection-service.md` with `#unbundle` node 7, when the
five RPCs it describes left `connection.ConnectionService` for `activity.ActivityService`. The two
*capture* seams that are still the daemon's are summarised there; everything below is this crate's.

## The log

A per-session log — `~/.tddy/sessions/{session_id}/agent-activity.jsonl` — records the **agent's
own** tool loop (Read, Shell/Bash, Edit, `tddy-tools` verbs), as opposed to the human-triggered
`ExecuteTool` invocations captured in `tool-calls.jsonl`. The record shape (`AgentActivityRecord`)
and the append/coalesce/500-cap read logic live in `tddy-core::agent_activity` so every host writes
the same format; see
[tddy-core architecture § Agent activity](../../tddy-core/docs/architecture.md#agent-activity-agent_activity).

### Capture — one seam per session type

| Session type | Who appends | Where |
|---|---|---|
| sandbox | the host-side executor `DaemonToolHandler::execute` — a `running` row then a terminal `completed`/`error` row around `tool_engine::execute_tool_with_env`, publishing each to the hub | `tddy-daemon` |
| claude-cli | the `PreToolUse`/`PostToolUse` hooks (`tddy-tools session-hook`) POST `ReportAgentActivity`; the daemon pairs Pre→Post per session | hook → this crate |
| tool / cursor-cli | the **coder participant** over LiveKit while the session is live — its presenter appends rows and broadcasts `PresenterEvent::AgentActivity`; the daemon serves the file snapshot over `/rpc` as fallback | `tddy-coder` |

## `AgentActivityHub`

`Mutex<HashMap<sessionId, broadcast::Sender<AgentActivityRecord>>>`, one broadcast channel per
session. It belongs to `tddy-daemon-kernel` (node 1's) and is consumed here unchanged — the sandbox
executor publishes into the same hub, so a second one in this crate would leave an in-jail tool call
invisible to every stream.

`StreamSessionActivity` mirrors the snapshot-then-live pattern `StreamTerminalOutput` /
`WatchTerminalControl` use: snapshot via `read_agent_activity`, then relay hub events with `Lagged`
handling. `ReportAgentActivity` and the sandbox executor are the publishers; the relay itself is
[`streams.rs`](../src/streams.rs)'s.

## Stream mode and payload

`StreamSessionActivityRequest.mode` (`StreamMode`, declared in `types.proto`) selects
`SNAPSHOT_THEN_LIVE` (default — replay then tail) or `LIVE_ONLY` (skip the snapshot, tail only
records arriving after subscribe); an unknown or omitted value falls back to `SNAPSHOT_THEN_LIVE`.

Record `input`/`result` are structured `google.protobuf.Value` on the wire (`serde_json::Value` in
`tddy-core`), mapped by `tddy_service::agent_activity_to_proto` + `json_to_proto_value`. The
claude-cli hook still sends `input_json`/`result_json` **strings**, parsed server-side (empty →
unset, else parse-or-string) via `tddy_core::agent_activity::parse_activity_json`.

## Cross-host limitation

`StreamSessionActivity` serves Local routes only and rejects `PeerRoute::Forward` with
`unimplemented`. The refusal is not this crate's: it lives in `tddy-daemon`'s `PeerRoutedActivity`
wrapper, because it is a statement about the transport's idle deadline — `forward_to_peer` is
unary-only, and `forward_server_stream_to_peer`'s deadline is sized for a short-lived stream.
Single-host, the common case, works fully. Feature:
[agent-activity-pane.md](../../../docs/ft/web/agent-activity-pane.md).

## The persisted ACP transcript and `StreamAcpReplay`

The session persists its own ACP-mapped conversation to `acp-transcript.jsonl` (sibling of
`agent-activity.jsonl`), written at event time by the coder participant's
`spawn_acp_transcript_writer` (consuming `presenter_events`: `AgentOutput` → agent-text frame,
`AgentActivity` → enriched tool frame via `tddy_service::acp_replay::frame_for_agent_activity`).

`StreamAcpReplay` re-emits that self-contained log (snapshot via `read_acp_transcript`) then tails
the hub — so the read-only web transcript renders for both live and dormant sessions without
depending on the agent-CLI-owned `conversation.jsonl`. Same `StreamMode` and same local-only routing
as `StreamSessionActivity`.

`TAIL_THEN_LIVE` replays only the newest `page_size` frames (default 100) via
`acp_replay::tail_page`, then tails; older history is reached backwards through `GetAcpReplayPage`.
Every transcript frame carries `seq`, its absolute 0-based position in the resolved transcript —
including `LIVE_ONLY`, which reads the transcript solely to establish that base, and excluding
`COUNT_THEN_LIVE`, which carries no transcript payload. The live tail maps `tool_call_id → seq`
(pre-seeded from the snapshot) so a call's terminal record reuses the position its running record
was given rather than consuming one of its own.

`GetAcpReplayPage` returns one page of frames strictly **older** than `before_seq`, oldest-first, with
`first_seq` (the client's next cursor) and an explicit `at_oldest` — a field rather than
`first_seq == 0`, because an *empty* page at the head would otherwise be indistinguishable from a
one-frame page at the head. A `before_seq` past the transcript end clamps to its length, so a stale
cursor resolves to a real page rather than nothing. It is unary precisely so it **peer-forwards**,
which the replay streams still cannot.

## Lazy tool bodies

Streamed tool frames carry only metadata (`title`/`status`/`kind`/`tool_call_id`);
`raw_input`/`raw_output` are cleared by `tddy_service::acp_replay::strip_tool_body` inside
[`streams::acp_replay_frame`](../src/streams.rs), covering the snapshot loop **and** the live
`relay_acp_replay` tail, in both `SNAPSHOT_THEN_LIVE` and `LIVE_ONLY` (`COUNT_THEN_LIVE`
unchanged). `GetAcpReplayPage` applies the same seam — a paged frame is not a back door to the
bodies.

The unary `GetAcpToolCallDetail` returns one call's bodies on demand via
`tddy_service::acp_replay::tool_call_detail(session_dir, tool_call_id)` (the same
`read_session_transcript` view), `NOT_FOUND` for an unknown id, with the same peer-forward routing as
`ExecuteTool`.

⚠ **The coder participant applies the identical strip in its own `replay_frame_bytes` and serves its
own `GetAcpToolCallDetail` arm.** The paging helpers are shared through `tddy-service`; the framing
and the `seq` stamping around them are written twice. Recorded in
[`docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md`](../../../docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md).

Feature: [acp-replay-lazy-tool-bodies.md](../../../docs/ft/coder/acp-replay-lazy-tool-bodies.md).

## Related

- [activity-service.md](./activity-service.md) — the eight methods and their ports
- [session-notifications.md](./session-notifications.md) — what `ReportAgentActivity` publishes onto
- [connection-service.md](../../tddy-daemon/docs/connection-service.md) — the capture seams that stayed
