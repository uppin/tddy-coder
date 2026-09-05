# 2026-03-28 — Session directory layout (unified `sessions/<id>/`)

- **Contract**: Plan and workflow state use `{sessions_base}/sessions/{session_id}/`; process-bound session id takes precedence over backend-reported ids where they differ (`tddy_core::session_lifecycle`).
- **Presenter**: The workflow runner resolves `session_dir` from engine context or materializes from `session_base` + `session_id`; missing both yields a clear workflow error (no anonymous fallback directory).
- **Docs**: [Session directory layout](../session-layout.md) (including [migration from non-unified trees](../session-layout.md#migration-from-non-unified-trees)).
