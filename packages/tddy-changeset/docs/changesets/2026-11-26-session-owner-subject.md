# 2026-11-26 — `.session.yaml` records who reached the session's target (#target-reconnect 2/3)

**Type:** Feature

`SessionMetadata.owner_subject: Option<String>` — the GitHub login whose vault reaches the session's `target`, written at target start and at resume when absent. A caller-less watcher, startup pass or lazy re-attach resolves the target as this subject; absent reads and writes as nothing, so host records and legacy target records are unchanged on disk. Part of #1041.
