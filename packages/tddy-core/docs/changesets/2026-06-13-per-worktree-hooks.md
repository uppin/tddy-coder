# 2026-06-13 — **Per-worktree hooks

**Type:** Feature

session activity status** — **`session_activity`**: **`SessionActivityStatus`** enum, `as_wire()`/`from_wire()`, **`activity_status_from_hook()`**, **`HookEvent`** serde; **`claude_hooks`**: **`HookCommandParams`**, **`build_claude_hooks_settings()`** (6-event settings JSON); **`session_metadata`**: `activity_status`/`hook_token` optional fields, **`update_activity_status()`** helper. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md#session-activity-status-via-per-worktree-hooks). (tddy-core)
