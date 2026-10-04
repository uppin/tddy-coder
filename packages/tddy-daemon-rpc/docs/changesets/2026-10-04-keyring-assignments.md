# 2026-10-04 — SetProjectAccounts handler

**Type:** Feature

`ProjectRpcHandler` serves `SetProjectAccounts`: authenticate, route local or forward to the peer
owning the `project_id`, refuse a repeated provider with `InvalidArgument`, replace the set. The
acceptance test `set_project_accounts_acceptance.rs` lives in this package's `tests/` because the
`ProjectService` handlers live here. `project/coordinate_handlers.rs` grew from 528 to 650
production lines; the split is deferred. See [architecture.md](../architecture.md).
