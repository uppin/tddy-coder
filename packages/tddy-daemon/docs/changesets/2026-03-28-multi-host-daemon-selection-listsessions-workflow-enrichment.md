# 2026-03-28 — Multi-host daemon selection + ListSessions workflow enrichment

**Type:** Feature

`ConnectionService`: `ListEligibleDaemons` via `EligibleDaemonSource`; `ListSessions` populates `daemon_instance_id` and enriched workflow fields (`session_list_enrichment`); `StartSession` honors local vs non-local `daemon_instance_id` (non-local returns unimplemented until peer routing); `spawn_blocking_with_timeout` for read + enrichment. Integration tests: `list_sessions_enriched`, multi-host. Feature docs: [connection-service.md](../connection-service.md), [docs/ft/web/web-terminal.md](../../../../docs/ft/web/web-terminal.md). (tddy-daemon, tddy-service, tddy-web)
