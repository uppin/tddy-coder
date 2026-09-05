# 2026-07-23 — session-activity-streaming-mode: `agent_activity::AgentActivityRecord.input`/`result` change from `String` to structured `serde_json::Value` (persisted un-nested in `agent-activity.jsonl`); new `parse_activity_json` (empty → `Null`, else parse-or-`Value::String`) shared by every capture seam; the presenter builds structured records. Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)

**Type:** Feature


