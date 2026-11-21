# 2026-11-21 — A claude-cli tool call is reported running before it runs

**Type:** Feature · **Stack:** `#target-parity` 4/7 (PR #972)

`remote_hook_report::reports_a_status`: the follower sends no session status for a `PreToolUse` line without an answer nonce, only the agent-activity report. Full record: [`docs/dev/changesets/2026-11-21-claude-cli-running-tool-calls.md`](../../../../docs/dev/changesets/2026-11-21-claude-cli-running-tool-calls.md).
