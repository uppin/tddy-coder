# Changeset: Remove the hardcoded `origin` remote assumption

**Date**: 2026-07-27
**Status**: ✅ Complete
**Type**: Bug Fix

## Affected Packages

- **tddy-core**: [README.md](../../packages/tddy-core/README.md)
  - [architecture.md](../../packages/tddy-core/docs/architecture.md) — validator + worktree helper descriptions: `<remote>/<path>` and main-worktree → config → `origin` resolution order.
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md)
  - (no `docs/` file changes — `project_storage` and `connection_service` behavior covered by the feature doc)
- **tddy-service**: [README.md](../../packages/tddy-service/README.md)
  - (proto field additions — covered by the feature doc)
- **tddy-web**: [README.md](../../packages/tddy-web/README.md)
  - (web picker normalization — covered by the feature doc)

## Related Feature Documentation

- [git-integration-base-ref.md](../ft/coder/git-integration-base-ref.md) — validation, unified-resolution, and default-remote-resolution sections updated in place (PRD, allowed direct edit).

## Summary

Removes the hardcoded `origin` remote assumption across validators, fetch/push/list, remote-ref resolution, and the web branch picker. `origin` survives only as the last-resort fallback when neither the main worktree's upstream nor the project config names a remote.

## Background

`validate_chain_pr_integration_base_ref` rejected any ref not starting with `origin/`, so a repo whose main worktree tracks a non-`origin` remote failed worktree setup with `chain PR integration base ref must start with origin/`. The string `origin` was hardcoded across validators, fetch, push, branch listing, remote-ref resolution, default-base probing, and the web picker normalizer.

## Scope

- [x] **Package Documentation**: feature doc updated; architecture.md delta described in this changeset for wrap-time application.
- [x] **Implementation**: code changes across tddy-core, tddy-daemon, tddy-service proto, tddy-web.
- [x] **Testing**: unit + acceptance tests for non-origin remotes, detection, and resolution order.
- [x] **Integration**: cross-package `default_remote` field threaded end-to-end.
- [x] **Technical Debt**: no fallbacks added; legacy `origin`-stripping helpers retained for backward-compat only.
- [x] **Code Quality**: `cargo fmt --check` + `clippy -- -D warnings` clean on touched crates; web build clean.

## Technical Changes

### State A (Current)

- `validate_integration_base_ref` / `validate_chain_pr_integration_base_ref` enforce an `origin/` prefix.
- `fetch_integration_base` / `fetch_chain_pr_integration_base` run `git fetch origin <branch>`.
- `resolve_default_integration_base_ref` runs `git fetch origin` then probes `origin/master` → `origin/main` → `refs/remotes/origin/HEAD`.
- `push_new_branch_to_origin` hardcodes `git push -u origin <branch>`.
- `list_recent_remote_branches*` filter lines starting with `origin/`.
- `local_branch_name` strips a leading `origin/`.
- `session_chain::resolve_chain_integration_base_ref_from_parent_session` builds `format!("origin/{trimmed}")`.
- `ProjectData` has no `remote_name` field; `effective_integration_base_ref_for_project` calls the bare resolver.
- `ProjectEntry` / `ListProjectBranchesResponse` carry no `default_remote`.
- Web `localBranchName` strips `origin/`; `ProjectsScreen` default-branch heuristic hardcodes `origin/master` → `origin/main`.

### State B (Target)

- Validators accept any safe `<remote>/<path>` (pure string rules, no git probe); error strings drop `origin/`.
- `detect_default_remote_name(repo_root) -> Option<String>` runs `git rev-parse --abbrev-ref @{upstream}` and splits on the first `/`; returns `None` on detached HEAD / missing upstream / git error.
- `resolve_default_integration_base_ref_with_remote(repo_root, preferred_remote)` chooses `preferred_remote` → detected → `origin`, then `git fetch <remote>` and probes `<remote>/master` → `<remote>/main` → `refs/remotes/<remote>/HEAD`. The bare `resolve_default_integration_base_ref` delegates with `None`.
- `fetch_integration_base` / `fetch_chain_pr_integration_base` split the validated ref on the first `/` into `(remote, path)` and run `git fetch <remote> <path>`.
- `push_new_branch_to_remote(worktree_dir, branch, remote)` runs `git push -u <remote> <branch>`; legacy `push_new_branch_to_origin` wraps it with `"origin"`.
- `list_recent_remote_branches*(repo_root, remote, ...)` filter lines starting with `<remote>/`, skipping `<remote>/HEAD`.
- `local_branch_name_for_remote(reference, remote)` strips one leading `<remote>/`; legacy `local_branch_name` strips `origin/`.
- `session_chain::resolve_chain_integration_base_ref_from_parent_session` resolves the child project's default remote (detect → config → `origin`) and builds `<remote>/<trimmed>`.
- `ProjectData.remote_name: Option<String>` (serde, skip-if-none); `effective_remote_name_for_project(projects_dir, project_id, repo_root)` resolves main-worktree → config → `origin`; `effective_integration_base_ref_for_project` calls `_with_remote(Some(&effective_remote_name_for_project(...)))` for legacy rows.
- `connection_service` threads the resolved remote through push, list, repoint, spawn-branch, and telegram call sites.
- `ProjectEntry.default_remote` (field 7) and `ListProjectBranchesResponse.default_remote` (field 2) populated from `effective_remote_name_for_project`.
- Web `localBranchName(reference, remote)` strips one leading `<remote>/`; `ProjectsScreen` uses `<default_remote>/master` → `<default_remote>/main`; `CreateSessionPane` passes `defaultRemote` into `localBranchName`.
- `DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF` renamed to `FALLBACK_DEFAULT_INTEGRATION_BASE_REF` with updated doc comment ("last resort when no remote can be detected").

### Delta (What's Changing)

#### tddy-core
- **Architecture**: worktree module — validators, detection, fetch/push/list/resolve, `local_branch_name`; session_chain — `<remote>/<branch>` construction.
- **API**: new `detect_default_remote_name`, `resolve_default_integration_base_ref_with_remote`, `local_branch_name_for_remote`, `push_new_branch_to_remote`; renamed constant `FALLBACK_DEFAULT_INTEGRATION_BASE_REF`.

#### tddy-daemon
- **API**: `ProjectData.remote_name`; `effective_remote_name_for_project`; `effective_integration_base_ref_for_project` wired to `_with_remote`.
- **Integration**: `connection_service` threads resolved remote through push/list/repoint/spawn-branch/telegram.

#### tddy-service
- **Proto**: `ProjectEntry.default_remote = 7`; `ListProjectBranchesResponse.default_remote = 2`.

#### tddy-web
- **API**: `localBranchName(reference, remote)`; `ProjectsScreen` default-branch heuristic uses `defaultRemote`; `CreateSessionPane` passes `defaultRemote` into `localBranchName`.

## Implementation Milestones

- [x] tddy-core validators + detection + fetch/push/list/resolve + `local_branch_name`.
- [x] tddy-core `session_chain.rs` builds `<remote>/<branch>` from resolved default remote.
- [x] tddy-daemon `ProjectData.remote_name` + `effective_remote_name_for_project` + `effective_integration_base_ref_for_project`.
- [x] tddy-daemon `connection_service.rs` threads resolved remote.
- [x] proto + codegen: `default_remote` fields populated.
- [x] tddy-web `localBranchName` remote-aware; `ProjectsScreen` + `CreateSessionPane` updated.
- [x] Tests updated for non-origin remotes; fluent tests added for detection + resolution order.
- [x] Docs: feature doc updated; this changeset created.

## Testing Plan

### Testing Strategy

**Primary Test Approach**: Unit + acceptance. The change is pure string validation plus git-command construction, so unit tests cover the validators and helpers; acceptance tests cover end-to-end resolution order and daemon threading.

### Coverage Requirements

- [x] Validator accepts `<remote>/<path>` for non-`origin` remotes (e.g. `upstream/feature/foo`).
- [x] Validator rejects refs with no remote segment (e.g. `refs/heads/main`) and unsafe characters.
- [x] `detect_default_remote_name` returns the tracked remote; returns `None` on detached HEAD.
- [x] `resolve_default_integration_base_ref_with_remote` probes `<remote>/master` then `<remote>/main`.
- [x] `effective_remote_name_for_project` prefers main worktree over config over `origin`.
- [x] `local_branch_name_for_remote` strips the given remote once.
- [x] Web `localBranchName` strips a non-`origin` remote prefix; `ProjectsScreen` picks the default branch under a non-`origin` remote.

## Acceptance Tests

### tddy-core
- [x] **Unit**: `worktree.rs` — validator, detection, resolution, `local_branch_name_for_remote`, `list_recent_remote_branches` for non-origin remotes.
- [x] **Acceptance**: `remote_branch_ref_acceptance.rs`, `resume_selected_branch_acceptance.rs`.

### tddy-daemon
- [x] **Acceptance**: `effective_spawn_branch_acceptance.rs`, `repoint_target_validation_acceptance.rs`, `project_default_branch_resolution_acceptance.rs`, `query_branch_resolution_acceptance.rs`, `set_project_default_branch_acceptance.rs`, `worktrees_rpc.rs`, `unified_worktree_base_acceptance.rs`.

### tddy-web
- [x] **Unit**: `branchNames.test.ts` — non-origin remote stripping.
- [x] **Component**: `ProjectsScreenAcceptance.cy.tsx` — non-origin default remote selection; `CreateSessionPane` normalization.

## Technical Debt & Production Readiness

- [x] No fallbacks added beyond the documented `origin` last-resort (consented in the plan).
- [x] Legacy `local_branch_name` / `push_new_branch_to_origin` retained as origin-fallback wrappers (documented, not test-gated branches).

## Decisions & Trade-offs

- **Validators stay pure string rules** (no git probe) — per user decision; a ref whose remote does not exist is accepted at the boundary and rejected later by `git fetch`.
- **Resolution order: main worktree → config → `origin`** — the main worktree upstream is the most authoritative signal; `origin` is last-resort only.
- **`effective_spawn_branch` takes a `remote: &str` parameter** (option (a) in the plan) for consistency, threaded from the four spawn paths that already resolve the project/repo.

## Refactoring Needed

None.

## Validation Results

### Change Validation

**Last Run**: 2026-07-27
**Status**: ✅ Passed

**Summary**:
- `cargo fmt --check` clean on touched crates.
- `clippy -- -D warnings` clean on touched crates.
- `bun run build` clean; `tsc --noEmit` no new errors in edited web source.

## References

- Feature doc: [git-integration-base-ref.md](../ft/coder/git-integration-base-ref.md).
- Plan: `plans/Remove hardcoded origin remote-7b56f8fe.plan.md`.
