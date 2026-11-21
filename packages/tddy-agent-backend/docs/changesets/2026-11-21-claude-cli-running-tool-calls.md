# 2026-11-21 — A claude-cli tool call is reported running before it runs

**Type:** Feature · **Stack:** `#target-parity` 4/7 (PR #972)

`build_claude_hooks_settings` installs a reporting `PreToolUse` group for every tool (matcher `""`) before the waiting `AskUserQuestion` group; an event may now hold several matcher groups. Full record: [`docs/dev/changesets/2026-11-21-claude-cli-running-tool-calls.md`](../../../../docs/dev/changesets/2026-11-21-claude-cli-running-tool-calls.md).
