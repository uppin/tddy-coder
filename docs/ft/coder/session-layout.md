# Session directory layout

## On-disk tree

Session state for the CLI, headless daemon (`tddy-service`), and `tddy-daemon` lives under a single canonical path:

`{sessions_base}/sessions/{session_id}/`

`sessions_base` is typically the user sessions root (for example `$HOME/.tddy`). The `sessions` segment is fixed (`SESSIONS_SUBDIR` in `tddy-core`). `session_id` is a single path segment (alphanumeric, `-`, `_`; no `/`, `\`, `..`, or other characters that would leave the subtree).

`changeset.yaml`, logs, workflow artifacts, and recipe-specific files under `artifacts/` sit inside that directory.

## Engine identity

When a backend reports a different agent thread id than the process-bound session id, the effective id for the workflow engine follows the **process-bound** session id. Policy lives in `tddy_core::session_lifecycle::resolve_effective_session_id`.

## RPC and daemon validation

`validate_session_id_segment` applies wherever a caller-supplied `session_id` joins `sessions_base`. Rejected ids surface as `INVALID_ARGUMENT` (gRPC) before any filesystem access. This matches delete-path rules and prevents path traversal via `session_id`.

## Headless daemon (`DaemonService`)

`GetSession` and `ListSessions` read only under `{sessions_base}/sessions/`. Each immediate child directory with a `changeset.yaml` is a listed session.

## Migration from non-unified trees

Deployments that store session data outside `{sessions_base}/sessions/{session_id}/` do not appear in list or get flows that use the unified contract. Manual relocation steps are documented in [session-layout migration](../../dev/1-WIP/session-layout-migration.md).

## Related documentation

- [Daemon ConnectionService](../../../packages/tddy-daemon/docs/connection-service.md) — RPCs that resolve session paths
- [Daemon project concept](../daemon/project-concept.md) — projects and `sessions_base` context
