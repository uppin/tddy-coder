# 2026-07-23 — session-activity-streaming-mode: `stream_session_activity` honours `StreamMode`

**Type:** Feature

`LIVE_ONLY` skips the snapshot replay (unknown/omitted → `SNAPSHOT_THEN_LIVE`); `report_agent_activity` parses the hook's `input_json`/`result_json` strings into structured `Value` (via `tddy_core::agent_activity::parse_activity_json`); the sandbox capture seam builds structured records; both map to the wire via `tddy_service::agent_activity_to_proto`. Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
