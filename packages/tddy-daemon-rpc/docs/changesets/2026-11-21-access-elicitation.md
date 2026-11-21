# 2026-11-21 — Access requests and granted writes

**Type:** Feature

- `exec_tool/path_access.rs`: `RequestPathAccess` for an orchestrator caller — reach check, request,
  `path_access` notification, wait.
- `exec_tool/orchestrator_reach.rs`: `orchestrator_read_reach`; `run_granted_write` runs an
  orchestrator's write inside one of its write grants, which `role_guard.rs` now admits.

See [architecture.md](../architecture.md). Full record: [`docs/dev/changesets/2026-11-21-access-elicitation.md`](../../../../docs/dev/changesets/2026-11-21-access-elicitation.md).
