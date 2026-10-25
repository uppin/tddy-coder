# Changeset: RefreshWorktreeDiskUsage recalculates sizes with progress

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 2 — one tool over the existing size calculator; progress read through `worktree-list`'s tools
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

`REFRESH_WORKTREE_DISK_USAGE` def, `ProjectHomePort::refresh_worktree_disk_usage`, failing tests.

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

- [ ] **Integration** `packages/tddy-daemon-rpc/tests/disk_refresh_acceptance.rs` — `refresh_returns_at_once_and_sizes_arrive_through_list_worktrees`: It returns `queued` N. Poll `ListWorktrees` until `sizing.done == total`; every row is CACHED with the walked byte count.
- [ ] **Integration** `same file` — `refreshing_one_path_recalculates_only_that_worktree`: The other rows' `size_calculated_at` is unchanged.
- [ ] **Integration** `same file` — `an_unknown_path_is_refused`: The error names the path (NOT_FOUND from the service).

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
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
