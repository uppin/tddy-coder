# 2026-06-25 — **Session inspector Tools tab

**Type:** Feature

durable tool-call log + ListSessionToolCalls handler** — new `tool_call_log.rs` module: `append_tool_call(session_dir, &record)` (JSONL append, `create_dir_all`), `read_tool_calls(session_dir)` (skip malformed lines, 500-entry tail cap); `execute_tool` handler appends a `ToolCallRecord` after every invocation (non-fatal); new `list_session_tool_calls` handler (auth → `validate_session_id_segment` → optional peer forward → `read_tool_calls` → map to `ToolCallInfo`); 5 unit tests + 6 acceptance tests. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-daemon)
