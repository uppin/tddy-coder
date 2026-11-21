# 2026-11-21 — A claude-cli tool call is reported running before it runs

**Type:** Feature · **Stack:** `#target-parity` 4/7 (PR #972)

`session-hook` reports no session status for a `PreToolUse` that does not wait (`--await-answer` absent); its agent-activity report still writes the running row. Full record: [`docs/dev/changesets/2026-11-21-claude-cli-running-tool-calls.md`](../../../../docs/dev/changesets/2026-11-21-claude-cli-running-tool-calls.md).
