# 2026-07-24 — activities-as-acp: new `session_participant::acp_transcript`

**Type:** Feature

`spawn_acp_transcript_writer` consumes `presenter_events` and appends event-time ACP frames to `acp-transcript.jsonl` (`AgentOutput`→agent-text, `AgentActivity`→enriched tool via `tddy_service::acp_replay::frame_for_agent_activity`), wired in `run.rs` (interactive + headless). The participant gains a `StreamAcpReplay` arm (persisted `read_acp_transcript` snapshot + live `presenter_events` tail, `AcpReplayFrame`-wrapped). Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-coder)
