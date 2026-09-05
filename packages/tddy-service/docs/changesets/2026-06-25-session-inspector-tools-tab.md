# 2026-06-25 — **Session inspector Tools tab

**Type:** Feature

connection.proto additions** — `ConnectionService` gains `ListSessionToolCalls` RPC; `ListSessionToolCallsRequest{session_token, session_id, daemon_instance_id}`, `ListSessionToolCallsResponse{repeated ToolCallInfo}`, `ToolCallInfo{task_id, tool_name, args_json, result_json, is_error, error_message, job_running, created_unix_ms}`; TypeScript codegen regenerated. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-service)
