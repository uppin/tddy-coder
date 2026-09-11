# 2026-08-29 — A roster agent's own turns are unobservable from the web

**Category:** Future enhancement
**Source:** session-agent-conversation-tab changeset, 2026-08-29

- The web can now hold *its own* conversation with an attached agent, but it still cannot replay the
  turns the **main agent** ran against it. There is no artifact to replay: a roster agent has no
  session directory, `StreamAcpReplay` resolves only `unified_session_dir_path(sessions_base,
  session_id)` (`packages/tddy-daemon/src/connection_service.rs:13518`), and the only non-test caller
  of `append_acp_frame` in the repo is the coder process
  (`packages/tddy-coder/src/session_participant/acp_transcript.rs:42`).
- The answer text exists only in the one-shot `mpsc` behind the `PromptAgentConversation` call that
  asked for it (`connection_service.rs:10256`) and is discarded after framing. What reaches a third
  party is a <=120-char `last_activity.summary` on the roster (`session_agent_status.rs:34`).
- Fix shape, all daemon-side: append a frame per agent turn under a synthesized per-agent transcript
  key, add an `agent_id` axis to `StreamAcpReplayRequest`
  (`packages/tddy-service/proto/connection.proto:1535-1542`, which has no such field), and implement
  the peer forward `stream_acp_replay` currently refuses (`connection_service.rs:13492` returns
  `UNIMPLEMENTED`) — a remote roster agent runs on another host, so without it the transcript would
  only ever work for local agents.

## Re-read at `#unbundle` node 7's wrap (2026-09-12)

Still open, unchanged in substance. Every coordinate in it moved, so the fix shape is now spread
across two crates rather than one file:

- `StreamAcpReplay` is `activity.ActivityService`'s, served by
  `packages/tddy-session-activity/src/service.rs`. It still resolves only
  `unified_session_dir_path(sessions_base, session_id)`.
- `StreamAcpReplayRequest` is declared in `packages/tddy-service/proto/activity.proto`, not
  `connection.proto`. It still has no `agent_id` field.
- The peer forward it refuses is now `tddy-daemon`'s
  `connection_service/svc_activity_ports.rs::stream_acp_replay`, which calls
  `refuse_if_addressed_at_a_peer` — so "implement the forward" means implementing it in
  `PeerRoutedActivity`, not in the serving crate, which deliberately cannot reach a peer.
- `PromptAgentConversation`'s one-shot `mpsc` is now in
  `packages/tddy-session-agents/src/service.rs`, and the 120-character `last_activity.summary` rule
  in `packages/tddy-session-agents/src/session_agent_status.rs`.
- The fix is therefore **no longer "all daemon-side"**: appending the per-agent frame is
  `tddy-session-agents`' (it is where the turn's text exists), reading it back is
  `tddy-session-activity`'s, and the peer forward is `tddy-daemon`'s. That is three owners where the
  entry assumed one — worth knowing before it is scheduled.
