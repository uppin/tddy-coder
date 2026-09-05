# 2026-07-22 — Agent-activity capture on the coder presenter + participant

**Type:** Feature

`presenter/presenter_impl.rs` appends an `AgentActivityRecord` on tool-use/tool-result and broadcasts `PresenterEvent::AgentActivity`; `session_participant/mod.rs` gains a `"StreamSessionActivity"` dispatch arm (clone of `StreamTerminalOutput`: snapshot from `agent_activity_path` + subscribe to the presenter broadcast), backed by the new `agent_activity_path` field on `connection_service_participant.rs`; `run.rs` builds `agent_activity_path` beside `tool_calls_path` and passes the presenter broadcast into `SessionConnectionService`. Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-coder)
