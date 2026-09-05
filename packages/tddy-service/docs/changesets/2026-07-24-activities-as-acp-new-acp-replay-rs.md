# 2026-07-24 — activities-as-acp: new `acp_replay.rs`

**Type:** Feature

the persisted `acp-transcript.jsonl` format (`serialize_frame`/`deserialize_frames`, `append_acp_frame`/`read_acp_transcript`) + ACP frame builders (`agent_text_frame`, `tool_use_frame`, `frame_for_agent_activity`; enriched title/`kind`/`raw_input` + `timestamp_unix_ms`). `proto/connection.proto` adds server-streaming `StreamAcpReplay` + `AcpReplayFrame { bytes acp_agent_message }` (ACP `AcpAgentMessage` as bytes — keeps the file self-contained); `proto/tddy/acp/v1/acp.proto` adds `timestamp_unix_ms` on `SessionNotification`; regenerated Rust + `tddy-web/src/gen`. Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
