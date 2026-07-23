# Changeset: session-worktree-inspector — Worktree tab (disk stats + clear/delete/restore)

**Date:** 2026-07-23
**Branch:** `feat-worktree-session-stats`
**Packages:** `tddy-service` (proto), `tddy-core`, `tddy-daemon`, `tddy-web`
**Feature PRD:** [docs/ft/web/session-worktree-inspector.md](../../ft/web/session-worktree-inspector.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Proto: add `CleanWorktree` + `RestoreSessionWorktree` RPCs and messages (`packages/tddy-service/proto/connection.proto`); regenerate Rust + TS (`src/gen/connection_pb.ts`)
- [x] `tddy-daemon::worktrees`: add `CleanWorktreeError` + `clean_worktree_under_repo(repo_root, worktree_path)` (membership gate, refuses primary, runs `git clean -fdx`)
- [x] `tddy-daemon` RPC: implement `clean_worktree` handler (auth → resolve main repo → `spawn_blocking` clean → invalidate stats cache → error mapping)
- [x] `tddy-daemon` RPC: implement `restore_session_worktree` handler (resolve session dir + project repo → `resolve_persisted_worktree_integration_base_for_session` + `setup_worktree_for_session_with_integration_base` → invalidate cache → return recreated path)
- [x] `tddy-web`: `useSessionWorktreeStats` hook (`src/rpc/useSessionWorktreeStats.ts`) — `refresh:false` on mount, `refresh:true` on 10-min `setInterval`, returns matching `WorktreeRow | null`
- [x] `tddy-web`: extract `formatDiskBytes` → `src/components/sessions/worktreeStatsFormat.ts` (shared with `WorktreesAppPage`)
- [x] `tddy-web`: `SessionWorktreeTab` component (`src/components/sessions/SessionWorktreeTab.tsx`) — stats rows + two-step Clear/Delete + missing-state Restore + Refresh
- [x] `tddy-web`: `InspectorTab` gains `"worktree"`; `InspectorTabs` gains Worktree button; `SessionInspectorDrawer` renders `SessionWorktreeTab`
- [x] `tddy-web`: register new test-ids in `cypress/support/testIds.ts`

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/SessionWorktreeTabAcceptance.cy.tsx` (5 tests, passing)
- [x] `packages/tddy-daemon/tests/worktree_session_actions_acceptance.rs` (3 tests, passing)

## Unit / integration tests

- [x] `packages/tddy-daemon/tests/worktrees_rpc.rs` — `CleanWorktree` + `RestoreSessionWorktree` RPC handler tests (6 tests: auth, arg validation, primary refusal, clear + cache invalidation, restore-from-changeset)

## Validation Results

### pr-wrap validation (2026-07-23)

**Critical: 0 · Warning: 0 · Info: 1**

- **Backend** (`worktrees.rs`, `connection_service.rs`, `connection_tonic_adapter.rs`): `clean_worktree_under_repo` and both handlers mirror the established `remove_worktree` pattern — auth → arg validation → project/repo resolution → `spawn_blocking` under `spawn_worker_request_timeout` → cache invalidation → typed error mapping (`map_clean_worktree_error`). No `unwrap`/`expect` on fallible paths, no hardcoding, no test-only branches. Security: clean is gated on `git worktree list` membership + primary refusal (no arbitrary path clean); `session_id` is validated with `validate_session_id_segment` (no path traversal). New TODO/FIXME: none.
- **Frontend** (`useSessionWorktreeStats.ts`, `SessionWorktreeTab.tsx`): hook loads cache-first (`refresh:false`) then polls `refresh:true` on the 10-min interval, cancels on unmount, matches on `repoPath`; component renders stats + two-step Clear/Delete + missing-state Restore. `formatDiskBytes` deduped into `worktreeStatsFormat.ts` (also consumed by `WorktreesAppPage`).
- **INFO** — `restore_session_worktree` maps all core `Result<_, String>` errors to `Status::internal`; a pre-existing worktree path would surface as `internal` rather than `failed_precondition`. Acceptable (restore is only offered when the worktree is missing).
- **Test-infra note:** the cadence acceptance test waits for the initial render before advancing `cy.clock()`, so the mount effect that registers the interval is committed first — no change to the shared `mountWithRpc` helper (an earlier such change regressed `SessionInspectorScreenSharingAcceptance` and was reverted).

## Delta summary

### `tddy-service` (proto)

- `ConnectionService` gains unary `CleanWorktree` and `RestoreSessionWorktree`.
- New messages: `CleanWorktreeRequest{session_token,project_id,worktree_path}`,
  `CleanWorktreeResponse{ok,message}`,
  `RestoreSessionWorktreeRequest{session_token,project_id,session_id}`,
  `RestoreSessionWorktreeResponse{ok,message,worktree_path}`.

### `tddy-core`

- No new public API — Restore reuses `resolve_persisted_worktree_integration_base_for_session` +
  `setup_worktree_for_session_with_integration_base`.

### `tddy-daemon`

- `worktrees.rs`: `enum CleanWorktreeError { GitFailed{message}, NotListed, CannotCleanPrimary, Io(String) }`
  and `pub fn clean_worktree_under_repo(repo_root, worktree_path) -> Result<(), CleanWorktreeError>`
  — parses `git worktree list`, refuses the first-listed (primary) row, runs `git clean -fdx` in the
  worktree path. Mirrors `remove_worktree_under_repo`.
- `connection_service.rs`: `clean_worktree` + `restore_session_worktree` handlers; both invalidate
  `worktree_stats_cache` on success. Clean reuses `main_repo_path_for_host` + `spawn_blocking` +
  timeout like `remove_worktree`; restore resolves the session directory and calls the core setup
  helper.

### `tddy-web`

- New: `src/rpc/useSessionWorktreeStats.ts`, `src/components/sessions/SessionWorktreeTab.tsx`,
  `src/components/sessions/worktreeStatsFormat.ts`.
- Changed: `InspectorTabs.tsx` (`"worktree"` tab + button), `SessionInspectorDrawer.tsx` (render
  `SessionWorktreeTab`), `WorktreesAppPage.tsx` (import shared `formatDiskBytes`), `testIds.ts`.
- Test scaffolding: `cypress/support/rpc/connectionServiceBackend.ts` gains `worktrees` scenario
  rows + `cleanWorktree` / `restoreSessionWorktree` / `listWorktreesForProject` / `removeWorktree`
  stubs and call counters; new page object `cypress/support/pages/sessionWorktreeTabPage.ts`.
