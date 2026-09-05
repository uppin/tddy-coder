# 2026-07-22 — `session-hook` reports agent activity

**Type:** Feature

for Claude Code `PreToolUse`/`PostToolUse`, `session_hook.rs` POSTs `ReportAgentActivity` with the tool payload (parsed via a private `HookToolPayload`, leaving `HookEvent`'s `Eq` intact), swallowing errors and exiting 0 (fail-quiet hook contract preserved). Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools)
