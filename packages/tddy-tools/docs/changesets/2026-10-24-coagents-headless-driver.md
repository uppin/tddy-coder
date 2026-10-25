# 2026-10-24 — A conversation's `approval_prompt` asks the daemon

**Type:** Feature

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). With `TDDY_CONVERSATION_ID` set, `PermissionServer` decides every permission prompt (but `mcp__tddy-tools__*`) through `tddy_session_tool_client::request_approval` on its own thread and runtime; a refusal, missing route, timeout or failure is a deny naming the reason. The tagged `TDDY_SOCKET` relay added earlier in this PR was removed before merge. Test: `tests/approval_prompt_deny_paths.rs` (3).
