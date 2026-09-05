# 2026-08-29 — `SessionContextDoc` says how to address a document

**Type:** Feature

new `relative_path` (field 8) carries a row's own basename for a stack-level manifest doc, `prs/<node_id>/<basename>` for a per-PR one, and `attachments/<basename>` for an attachment; the client-side reconstruction it replaces could not express a nested path. Also corrects two stale comments: `SESSION_ARTIFACT` resolves a full relative path rather than a bare basename, and attachments materialize under `{session_dir}/artifacts/attachments/`, which is what every code path already did.
