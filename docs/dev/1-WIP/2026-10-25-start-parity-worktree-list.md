# Changeset: ListWorktrees and GetWorktree match the Worktrees screen

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — tddy-tool-engine + tddy-daemon-rpc + a tddy-web parity spec, an I/O seam (the worktree service and size calculator)
**Stack**: `#start-parity` 3/9 — branch `feature/start-parity/worktree-list`, base `n2` `start-tool` (`feature/start-parity/start-tool`)
**PRD**: [PRD-2026-10-25-start-parity-worktree-list.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-worktree-list.md)

## Initial Discovery

See [./2026-10-25-start-parity-worktree-list-initial-discovery.md](./2026-10-25-start-parity-worktree-list-initial-discovery.md).

## Responsibility

- `ListWorktrees` and `GetWorktree`: their defs, port methods and daemon implementation.
- The shared snapshot read.
- The Rust sort that matches `sortWorktreesBySize`, and the parity fixtures for fields and order.

## Boundaries

- Does not trigger a size calculation (that is `disk-refresh`).
- Does not expose Clear, Delete or Restore. They are destructive and nobody asked for them;
  recorded as a TODO.
- Does not add forwarding to `WorktreeService`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` `remote-targets` (`feature/start-parity/remote-targets`) | the delegated home caller, the `daemon_instance_id` target argument, and the forward of a whole project-home tool call to the target's host | its tool takes `daemon_instance_id` and is forwarded by n1's route; one cross-host test drives it | add a forward, a delegated-caller check, or a per-tool peer call |

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

- `LIST_WORKTREES` and `GET_WORKTREE` defs.
- `ProjectHomePort::{list_worktrees, get_worktree}`.
- `WorktreeListing` and `sort_worktrees_by_size`.
- Fixtures `packages/tddy-web/cypress/fixtures/worktree-tool-fields.json` and
  `worktree-sort-order.json`.
- Failing tests.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its tests exercise `n1` `remote-targets` (`feature/start-parity/remote-targets`)'s behaviour, which must be green first
**Concurrent with:** `n4` `async-start` (`feature/start-parity/async-start`), `n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`)
**Blocks:** `n8` `disk-refresh` (`feature/start-parity/disk-refresh`)

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n4` `async-start` (`feature/start-parity/async-start`), `n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`), `n8` `disk-refresh` (`feature/start-parity/disk-refresh`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`
- [ ] `tddy-web (parity spec only)`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-worktree-list.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-worktree-list.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

The orchestrator cannot see the project's worktrees as the Worktrees screen (#730) shows them.
This node adds **`ListWorktrees`**, a paged list with the screen's fields and its sort (largest
first), and **`GetWorktree`**, one worktree's detail as the screen's detail view shows it.

## Scope

- [ ] An orchestrator can list the project's worktrees, page by page, largest first, with the screen's fields.
- [ ] It can read one worktree's detail as the screen shows it.
- [ ] A field added to the screen's row that the tool lacks fails a test.

## Technical Changes

### State A

- The screen streams `WorktreeService.StreamWorktreeStats{project_id, recalculate_all}`
  (`WorktreeRow`: path, branch_label, disk_bytes, changed_files, lines_added, lines_removed,
  updated_at_unix_ms, stale, size_status, size_calculated_at_unix_ms).
- It sorts with `sortWorktreesBySize` (`tddy-web/src/lib/worktreeList.ts:34-44`) and does not page.
- Its detail view (`SessionWorktreeTab.tsx`) shows disk, last calculated, branch and changed files.
- Server: `tddy-worktree-service/src/service.rs:630` (snapshot built from `list_worktree_diff_rows`
  plus `WorktreeSizeCalculator::state`). It has no peer forwarding.

### State B

- `ListWorktrees {cursor?, limit?, daemon_instance_id?}` → `{worktrees: [WorktreeRow…], total,
  next_cursor, has_more, sizing: {done, total}}`.
  - Sorted by the screen's rule: `disk_bytes` descending, unsized last, then name.
  - `limit` has a default of 50 and a hard maximum of 200.
- `GetWorktree {path, daemon_instance_id?}` → the full `WorktreeRow` plus the session that owns it,
  if any.
- Both read the same snapshot `StreamWorktreeStats` sends first, through one shared function
  (no second listing path), and are limited to the caller's project.
- Another host is reached through `remote-targets`' forward.

## Testing Plan

### Acceptance Tests

- [ ] **Unit** `packages/tddy-tool-engine/tests/worktree_tools_parity.rs` — `list_worktrees_returns_every_worktree_row_field`: Every `tddy.v1.WorktreeRow` field in the descriptor is in the tool's row schema.
- [ ] **Cypress component** `packages/tddy-web/cypress/component/worktreeToolParity.cy.ts` — `the tool sorts worktrees exactly as the Worktrees screen does`: `sortWorktreesBySize` on the fixture input equals the fixture's Rust-produced order.
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/worktree_tools_acceptance.rs` — `list_worktrees_pages_largest_first_with_a_total`: Three worktrees and `limit` 2: page 1 holds the two largest, `has_more`, total 3; page 2 holds the third.
- [ ] **Integration** `same file` — `a_limit_above_the_maximum_returns_at_most_the_maximum`: `limit` 10000 returns ≤ 200.
- [ ] **Integration** `same file` — `get_worktree_returns_the_detail_the_screen_shows`: branch_label, disk_bytes, changed_files, size_status and size_calculated_at equal the stream's row for that path.
- [ ] **Integration** `packages/tddy-daemon/tests/project_home_remote_targets_acceptance.rs` — `list_worktrees_lists_another_hosts_worktrees`: With `daemon_instance_id` = the peer, the peer's worktrees are returned.

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **No new listing path.** The tool reads the snapshot the stream's first frame is built from, so
  the screen and the tool cannot disagree.
- **The sort is mirrored in Rust and pinned by a shared fixture**, not reimplemented by guesswork.

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

- [x] Record initial discovery (`2026-10-25-start-parity-worktree-list-initial-discovery.md`)
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
