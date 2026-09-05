# 2026-07-12 — **Session usage streaming

**Type:** Feature

`gather_session_usage` + `usage_watcher`** — `PresenterEvent::TokenUsageUpdated(Vec<ConversationRecord>)`; `backend::gather_session_usage()` merges main transcript + Claude Task subagents + `egress/accounting.json` into one ordered snapshot (extracted from `tddy-sandbox-app`'s `print_token_summary`, now reused); new `usage_watcher` module — `SessionUsageEmitter` (broadcast + dedup), poll-based `spawn_usage_watcher`, session-level `spawn_session_usage_watcher` (derives `include_main_agent` from the agent) — moved here from `tddy-daemon` so `tddy-coder` can call it. Feature [session-usage-inspector.md](../../../../docs/ft/web/session-usage-inspector.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#295](https://github.com/uppin/tddy-coder/pull/295). (tddy-core)
