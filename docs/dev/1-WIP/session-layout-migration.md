# Session layout migration (unified `sessions/<id>/`)

See the product-oriented description: [Session directory layout](../../ft/coder/session-layout.md).

## Current contract

On-disk state for a session lives under:

`{sessions_base}/sessions/{session_id}/`

Examples: `changeset.yaml`, logs, and workflow artifacts. CLI, `tddy-service` daemon mode, and `tddy-daemon` all assume this tree.

## Legacy flat layouts

Older or manual setups may have stored session data **directly** under `{sessions_base}/` (or elsewhere) **without** the `sessions/` segment, or used ad hoc directory names.

Those directories are **not** discovered by `ListSessions` / `GetSession`, which only scan and resolve paths under `{sessions_base}/sessions/`.

## Upgrade path

1. For each legacy session directory `L` that should remain active, choose the canonical `session_id` (single path segment: alphanumeric, `-`, `_`; no `/`, `\`, or `..`; see `tddy_core::session_lifecycle::validate_session_id_segment`).
2. Create `{sessions_base}/sessions/{session_id}/`.
3. Move the contents of `L` into that directory (preserve `changeset.yaml` and relative paths as needed).
4. Restart clients so they use the same `session_id` string.

Automated migration tooling is not part of this changeset; perform the above manually or with a one-off script in a controlled maintenance window.

## Security note

Always use validated `session_id` strings when constructing paths. APIs reject malformed ids so they cannot escape the `sessions/` subtree via `..` or path separators.
