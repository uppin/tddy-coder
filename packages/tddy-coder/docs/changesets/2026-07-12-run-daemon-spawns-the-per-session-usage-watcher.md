# 2026-07-12 — `run_daemon` spawns the per-session usage watcher

**Type:** Feature

in the LiveKit block, `spawn_session_usage_watcher` runs against the session's egress dir / transcript with the session's presenter `event_tx`, so a running session broadcasts live `TokenUsageUpdated` (→ `event_to_server_message` → `TddyRemote.Stream` → web Inspector); the watcher task is aborted on shutdown. `claude_home` resolved from `HOME`/`USERPROFILE`. Feature [session-usage-inspector.md](../../../../docs/ft/web/session-usage-inspector.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#295](https://github.com/uppin/tddy-coder/pull/295). (tddy-coder)
