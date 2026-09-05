# 2026-06-25 — Session inspector Tools tab

**Type:** Feature

`connection.proto`: `ListSessionToolCalls` RPC + `ToolCallInfo` message; `tddy-daemon`: `tool_call_log.rs` (JSONL append log at `~/.tddy/sessions/{id}/tool-calls.jsonl`, 500-entry tail cap), `execute_tool` durably logs every call (non-fatal), `list_session_tool_calls` handler; `tddy-web`: `InspectorTabs` Details/Tools strip, `SessionToolsTab` (invoke panel + collapsible call log with stdio parsing), `toolSchema.ts` (`defaultArgsFromSchema`), 17 new Cypress tests. Feature [session-drawer.md](../ft/web/session-drawer.md); PR [#226](https://github.com/uppin/tddy-coder/pull/226). (tddy-service, tddy-daemon, tddy-web)
