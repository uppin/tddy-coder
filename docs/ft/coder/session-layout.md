# Session directory layout

## On-disk tree

Session state for the CLI, headless daemon (`tddy-service`), and `tddy-daemon` lives under a single canonical path:

`{sessions_base}/sessions/{session_id}/`

`sessions_base` is typically the user sessions root (for example `$HOME/.tddy`). The `sessions` segment is fixed (`SESSIONS_SUBDIR` in `tddy-core`). `session_id` is a single path segment (alphanumeric, `-`, `_`; no `/`, `\`, `..`, or other characters that would leave the subtree).

`changeset.yaml`, logs, workflow artifacts, and recipe-specific files under `artifacts/` sit inside that directory.

## Engine identity

When a backend reports a different agent thread id than the process-bound session id, the effective id for the workflow engine follows the **process-bound** session id. Policy lives in `tddy_core::session_lifecycle::resolve_effective_session_id`.

## Codex session id (`codex_thread_id`)

For **`--agent codex`** and **`--agent codex-acp`**, the workflow persists the agent’s thread / session identifier in **`codex_thread_id`** under the session directory. **`codex-acp`** resume uses ACP **`load_session`** with that same stored value. Product reference: [codex-acp-backend.md](codex-acp-backend.md).

## RPC and daemon validation

`validate_session_id_segment` applies wherever a caller-supplied `session_id` joins `sessions_base`. Rejected ids surface as `INVALID_ARGUMENT` (gRPC) before any filesystem access. This matches delete-path rules and prevents path traversal via `session_id`.

## Headless daemon (`DaemonService`)

`GetSession` and `ListSessions` read only under `{sessions_base}/sessions/`. Each immediate child directory with a `changeset.yaml` is a listed session.

## Migration from non-unified trees

Deployments that store session data outside `{sessions_base}/sessions/{session_id}/` do not appear in list or get flows that use the unified contract.

### Legacy flat layouts

Older or manual setups may have stored session data **directly** under `{sessions_base}/` (or elsewhere) **without** the `sessions/` segment, or used ad hoc directory names.

Those directories are **not** discovered by `ListSessions` / `GetSession`, which only scan and resolve paths under `{sessions_base}/sessions/`.

### Manual upgrade path

1. For each legacy session directory `L` that should remain active, choose the canonical `session_id` (single path segment: alphanumeric, `-`, `_`; no `/`, `\`, or `..`; see `tddy_core::session_lifecycle::validate_session_id_segment`).
2. Create `{sessions_base}/sessions/{session_id}/`.
3. Move the contents of `L` into that directory (preserve `changeset.yaml` and relative paths as needed).
4. Restart clients so they use the same `session_id` string.

Automated migration tooling is not part of the product; perform the above manually or with a one-off script in a controlled maintenance window.

### Security note

Always use validated `session_id` strings when constructing paths. APIs reject malformed ids so they cannot escape the `sessions/` subtree via `..` or path separators.

## Related documentation

- [Daemon ConnectionService](../../../packages/tddy-daemon/docs/connection-service.md) — RPCs that resolve session paths
- [Daemon project concept](../daemon/project-concept.md) — projects and `sessions_base` context
