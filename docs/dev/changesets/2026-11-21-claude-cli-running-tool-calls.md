# 2026-11-21 — A claude-cli tool call is reported running before it runs

**Type:** Feature · **Stack:** `#target-parity` 4/7 (PR #972) · **Packages:** `tddy-agent-backend`,
`tddy-tools`, `tddy-agent-launch` · Feature:
[claude-cli-session.md § Hook wiring](../../ft/daemon/claude-cli-session.md#hook-wiring)

## Problem

`PreToolUse` was installed only for `AskUserQuestion`, and on a target not at all. Every activity row
was written by `PostToolUse` with `started_unix_ms: 0`, so no call was ever seen running.

## What changed

- **`tddy-agent-backend`**: `CLAUDE_HOOK_EVENTS` adds `("PreToolUse", "")`; `build_claude_hooks_settings`
  accumulates an event's matcher groups in order (reporting `""` first, then the waiting
  `AskUserQuestion` where installed). A target gets the reporting group as its log command.
- **`tddy-tools`**: `session_status_request` returns no status for a `PreToolUse` that does not
  wait (`await_answer = false`).
- **`tddy-agent-launch`**: `remote_hook_report::reports_a_status` — the follower's `ActivityHookSink`
  sends no status for a `PreToolUse` line without an answer nonce.

## Decisions

- **Local too** (user, 2026-11-21): one extra short `tddy-tools` process per tool call locally, in
  exchange for the same running-call behaviour locally and on a target.
- **Activity-only catch-all** rather than a matcher regex excluding the question tools.

## Tests

`remote_hooks_acceptance::a_tool_call_of_an_agent_on_a_target_is_reported_before_it_runs`,
`remote_hooks_render_acceptance::claude_code_on_a_target_reports_every_tool_before_it_runs`,
`session_hook_pending_question::a_pre_tool_use_that_does_not_wait_reports_no_status`, and units in
`claude_hooks.rs` and `remote_hook_follower.rs`.
