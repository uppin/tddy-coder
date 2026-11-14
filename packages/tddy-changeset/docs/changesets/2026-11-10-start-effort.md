# 2026-11-10 — `SessionMetadata.effort`

**Type:** Feature

- `SessionMetadata.effort: Option<String>`: the level a session was started at, for a resume. Never written empty, so a record without one still reads on an older daemon (which refuses unknown fields).

Cross-package: [effort on session start](../../../../docs/dev/changesets/2026-11-10-start-effort.md).
