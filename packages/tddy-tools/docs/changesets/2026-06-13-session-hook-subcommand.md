# 2026-06-13 — **`session-hook` subcommand

**Type:** Feature

per-worktree activity status reporter** — reads stdin hook JSON, maps event via `tddy_core::activity_status_from_hook`, POSTs `ReportSessionStatus` via connect-protocol; fail-quiet (always exit 0, 2s timeout); `encode_resize()` corrected to OSC `\x1b]resize;{cols};{rows}\x07`; `#[cfg(feature = "livekit")]` gate on encode_resize. Tests: `session_hook_cli` (5 tests). Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md#session-activity-status-via-per-worktree-hooks). (tddy-tools)
