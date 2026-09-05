# 2026-03-22 — Toolcall submit immediate wire acknowledgment

**Type:** Feature

`start_toolcall_listener`: `submit` returns `SubmitOk` on the socket right after `store_submit_result`; `try_send` of `SubmitActivity` for activity log only. `ToolCallRequest::SubmitActivity`; ask/approve still use oneshot until `poll_tool_calls`. Tests: `toolcall_relay_presenter_stuck`. (tddy-core, tddy-tools, tddy-coder tests)
