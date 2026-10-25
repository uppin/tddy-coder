# Changeset: RefreshWorktreeDiskUsage recalculates sizes with progress

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 2 — one tool over the existing size calculator; progress read through `worktree-list`'s tools
**PR**: [#757](https://github.com/uppin/tddy-coder/pull/757)
**Stack**: `#start-parity` 8/9 — branch `feature/start-parity/disk-refresh`, base `n7` `terminate` (`feature/start-parity/terminate`)
**PRD**: [PRD-2026-10-25-start-parity-disk-refresh.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-disk-refresh.md)

## Initial Discovery

See [./2026-10-25-start-parity-disk-refresh-initial-discovery.md](./2026-10-25-start-parity-disk-refresh-initial-discovery.md).

## Responsibility

The `RefreshWorktreeDiskUsage` tool, its port method and its daemon implementation.

## Boundaries

Does not change the calculator or its concurrency.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n3` `worktree-list` (`feature/start-parity/worktree-list`) | `ListWorktrees` / `GetWorktree` and their `sizing` progress | progress is read through them | add a second listing path or change their row shape |

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

- `tddy-tool-engine::project_home_disk_refresh`: `REFRESH_WORKTREE_DISK_USAGE`,
  `RefreshQueued {queued}`, and `refresh_worktree_disk_usage_tool_def()` (`TODO(disk-refresh)`).
  It is a module of its own because `project_home_worktree_tools` is #752's.
- `ProjectHomePort::refresh_worktree_disk_usage(project_id, path: Option<&str>)`, with a default
  body that bails `TODO(disk-refresh)`.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — its tests exercise `n3` `worktree-list` (`feature/start-parity/worktree-list`)'s behaviour, which must be green first
**Concurrent with:** `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)
**Blocks:** nothing

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-disk-refresh.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-disk-refresh.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

The orchestrator can read worktree sizes but cannot refresh them. This node adds
**`RefreshWorktreeDiskUsage`**, which triggers the refresh the Worktrees screen triggers.

## Scope

- [ ] An orchestrator can refresh one worktree's size, or all of them, and follow the progress the screen shows.

## Technical Changes

### State A

- The screen's Refresh re-subscribes `StreamWorktreeStats{recalculate_all: true}`. The
  per-worktree Refresh calls `CalculateWorktreeSize`, which returns at once.
- `WorktreeSizeCalculator` (`tddy-worktree-service/src/worktrees.rs:424-600`) sizes in the
  background (two walks at a time), keeps NONE/CALCULATING/CACHED per path, and persists the
  results.

### State B

`RefreshWorktreeDiskUsage {path?, daemon_instance_id?}` enqueues one worktree, or every
worktree when no path is given, through the calculator's own `enqueue`, and returns `{queued}`
at once. Progress is read with `ListWorktrees` (its `sizing: {done, total}` is the screen's
"{done} of {total}") or with `GetWorktree` (`size_status`). No new ledger is added: the
calculator's state is the record.

## Testing Plan

### Acceptance Tests

- [ ] **Unit** `packages/tddy-tool-engine/tests/disk_refresh_schema.rs`:
  - `it_refreshes_one_worktree_or_all_and_says_where_progress_is_read`
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/disk_refresh_acceptance.rs` (two hosts;
  progress is read through #752's `ListWorktrees`):
  - `a_refresh_returns_at_once_and_every_size_arrives_through_list_worktrees`
  - `refreshing_one_worktree_recalculates_only_it` — grows one worktree by 512 KiB and checks
    that the other's `size_calculated_at` is unchanged
  - `an_unknown_worktree_is_refused_naming_it`
  - `another_hosts_worktrees_are_refreshed_there`

Tests are written in the `fluent-tests` style (Given / When / Then). 

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **No ledger.** The calculator already holds a persisted state per path, which the screen's progress is computed from.

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

## TODO

- [x] Record initial discovery (`2026-10-25-start-parity-disk-refresh-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer: "finish the remaining nodes without stopping", 2026-10-25)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
