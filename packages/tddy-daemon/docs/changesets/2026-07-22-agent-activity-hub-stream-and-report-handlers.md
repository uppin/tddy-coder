# 2026-07-22 — Agent-activity hub, stream, and report handlers

**Type:** Feature

new **`AgentActivityHub`** (`Mutex<HashMap<sessionId, broadcast::Sender<AgentActivityRecord>>>`); `StreamSessionActivity` (snapshot via `read_agent_activity` then relay hub events with `Lagged` handling, mirroring `StreamTerminalOutput`; **Local-only**, `PeerRoute::Forward` → `unimplemented`); `ReportAgentActivity` (append + publish, auth like `report_session_status`, Pre→Post coalescing, bad-token rejection). `sandbox_session.rs`: `DaemonToolHandler::execute` threads `session_dir` + hub and appends running/terminal rows. `connection_tonic_adapter.rs` binds the new stream type. Doc [connection-service.md § Agent-activity log & stream](../connection-service.md#agent-activity-log--stream). Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
