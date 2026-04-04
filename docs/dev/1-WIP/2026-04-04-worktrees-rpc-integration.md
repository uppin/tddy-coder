# Changeset: Worktrees ConnectionService + web (local daemon)

**Date**: 2026-04-04  
**Status**: Implemented (wrap when merged)  
**Type**: Feature

## Affected packages

- `tddy-service` — `connection.proto`
- `tddy-daemon` — `connection_service`, `WorktreeStatsCache` wiring
- `tddy-web` — `WorktreesAppPage`, generated `connection_pb`

## Plan reference

Implements `.cursor/plans/Worktrees RPC web integration-23f61554.plan.md` (do not edit plan file).

## Scope

- [x] Proto + codegen (Rust + TS)
- [x] Daemon RPC handlers + integration tests (`tests/worktrees_rpc.rs`)
- [x] Web data loading + actions (`WorktreesAppPage`)
- [x] Feature / package docs + changelogs
- [x] `./verify`, `cargo clippy --workspace -- -D warnings`, `bun run build` (tddy-web)

## Technical notes

- **Cache**: Single `Arc<WorktreeStatsCache>` per `ConnectionServiceImpl`, root `projects_stats_cache_root()`.
- **Project path**: `project_storage::main_repo_path_for_host(projects_dir, project_id, local_daemon_instance_id)`.
- **Refresh**: `ListWorktreesForProjectRequest.refresh` → `refresh_stats_for_project` in `spawn_blocking`.
- **Remove**: `invalidate_project` after successful remove; `RemoveWorktree` uses `tokio::task::spawn_blocking` + timeout (same wall-clock cap as other blocking RPCs).

### Validation results

- **2026-04-04**: Full `cargo test` via `./verify` (see `.verify-result.txt`); workspace clippy `-D warnings`; `packages/tddy-web` production build succeeded.
