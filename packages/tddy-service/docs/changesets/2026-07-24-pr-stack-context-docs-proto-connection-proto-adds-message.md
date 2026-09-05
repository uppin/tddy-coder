# 2026-07-24 — pr-stack-context-docs: `proto/connection.proto` adds `message SessionContextDoc { key, basename, path, description, exists }` and `repeated SessionContextDoc context_docs = 27` on `SessionEntry` (regenerated Rust). Consumed by the daemon's `session_context_docs` surface + `ListSessions` enrichment; the `ReadSessionContextDoc` RPC is a stacked follow-up. Feature [pr-stacking.md](../../../../docs/ft/coder/pr-stacking.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)

**Type:** Feature


