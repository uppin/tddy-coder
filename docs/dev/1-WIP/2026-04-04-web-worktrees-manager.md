# Changeset: Web Worktrees manager (library-first)

**Date**: 2026-04-04  
**Status**: Complete (implementation + docs; verify `git status` before push)  
**Type**: Feature

## Affected packages

- `tddy-daemon`
- `tddy-web`
- `docs` (feature and dev)

## Related feature documentation

- [docs/ft/web/worktrees.md](../../ft/web/worktrees.md)
- [docs/ft/web/web-terminal.md](../../ft/web/web-terminal.md)

## Summary (State B)

The repository includes a **`tddy_daemon::worktrees`** library: **`git worktree list`** parsing, persisted per-project stats under **`TDDY_PROJECTS_STATS_ROOT`**, lexical path policy helpers, and **`git worktree remove`** for non-primary worktrees. **`tddy-web`** ships a **`WorktreesScreen`** component and Cypress coverage with mocked data. **ConnectionService** has no worktree RPCs; shell hamburger navigation and daemon-backed listing are outside this milestone.

## Scope

- [x] Daemon `worktrees` module with tests (`worktrees`, `worktrees_acceptance`)
- [x] Web `WorktreesScreen` + component test
- [x] Feature and package documentation, changelogs, cross-package dev changeset index

## Implementation Progress

**Last synced with code**: 2026-04-04 (via `@validate-changes`)

**Core features**

- [x] Daemon `worktrees` library — complete (`packages/tddy-daemon/src/worktrees.rs`, `lib.rs` export)
- [x] Unit tests — complete (`worktrees.rs` `#[cfg(test)]`: parser + path policy)
- [x] Integration tests — complete (`packages/tddy-daemon/tests/worktrees_acceptance.rs`: cache + remove; requires `git` on PATH)
- [x] Web UI — complete (`WorktreesScreen.tsx`, `WorktreesScreen.cy.tsx` harness)
- [x] Docs — complete (feature + package docs and changelogs on branch)

**Repo hygiene (pre-commit)**

- Untracked or unstaged files may still exist for the implementation paths above; stage and commit so `master...HEAD` includes all sources.

### Change Validation (@validate-changes)

**Last run**: 2026-04-04  
**Status**: Passed (with notes)  
**Risk level**: Low

**Changeset sync**

- Aligned with working tree and session `019d576e-8549-7170-a17e-a09a19e5af09` (state `DocsUpdated`).

**Build / lint**

- `cargo build -p tddy-daemon`: success  
- `cargo clippy -p tddy-daemon -- -D warnings`: success  
- `cargo test -p tddy-daemon` (worktrees unit filter + `--test worktrees_acceptance`): success

**Analysis summary**

- Packages built: `tddy-daemon`  
- Files reviewed: `worktrees.rs`, `worktrees_acceptance.rs`, `WorktreesScreen.tsx`, `WorktreesScreen.cy.tsx`, `lib.rs`  
- Production risks: low; no secrets; path policy and primary-worktree guard for remove  
- Test infrastructure: `WorktreeStatsCache` exposes **`test_git_diff_invocations`** for acceptance tests (documented in `packages/tddy-daemon/docs/worktrees.md`); list path does not invoke diff/stat

**Risk assessment**

- Build: low  
- Test infrastructure: low (documented test hooks)  
- Production code: low  
- Security: low (local git/fs; no network RPC in this milestone)  
- Code quality: low  

### Refactoring / follow-ups (from @validate-changes)

- [ ] Optional: when ConnectionService RPCs land, relocate or gate **`test_git_diff_invocations`** on `WorktreeStatsCache` if it should not ship in production builds.

### PR wrap validation (2026-04-04)

| Step | Result |
|------|--------|
| `@validate-changes` | Helpers extracted; integration test renamed to **`worktrees_acceptance.rs`**; removed unused **`test_git_diff_on_list_calls`** |
| `@validate-tests` | Daemon + Cypress patterns OK; **`git`** required for integration tests; integration test renamed **`remove_worktree_drops_listing_and_repeat_fails`** |
| `@validate-prod-ready` | No mock daemon code in production paths; **`WorktreesScreen`** mock props documented |
| `@analyze-clean-code` | **`refresh_stats_for_project`** split into **`git_worktree_list_stdout`**, **`build_worktree_stat_snapshots`**, **`write_worktree_stats_cache_file`** |
| Final `@validate-changes` | Aligned with refactors below |
| **`cargo fmt`** + **`cargo clippy -- -D warnings`** + **`./dev ./verify`** | Passed; evidence: **`.verify-result.txt`** (workspace root) |
| Cross-cutting test fixes (non–worktrees) | **`grpc_full_workflow`**: assert fixed **suffix** from **`AcceptanceTesting` → `DocsUpdated`** + variable Planning prefix. **`acp_raw_pipe_initialize`**: read lines until **`jsonrpc`** appears. |

### Documentation wrap (`@wrap-context-docs`)

- Feature and package docs already describe **State B** (**`docs/ft/web/worktrees.md`**, **`packages/tddy-daemon/docs/worktrees.md`**, changelogs, **`docs/dev/changesets.md`**, **`packages/tddy-daemon/docs/changesets.md`**).
- This WIP file is retained as the PR audit trail; remove from **`docs/dev/1-WIP/`** only after merge if your process deletes wrapped changesets.

## Follow-up (not in this changeset)

- `connection.proto` RPCs and `connection_service` handlers
- Background refresh scheduler with bounded concurrency
- Canonical path policy where symlinks matter
- Shell route and `ListEligibleDaemons` wiring for Worktrees

## References

- Package: [packages/tddy-daemon/docs/worktrees.md](../../../packages/tddy-daemon/docs/worktrees.md)
- Evaluation artifacts (when present): `workflow-free-prompting-validate/` reports (parallel validation run; distinct topic)
