# Changeset: Clean, Delete and Restore of a target session act on the target worktree

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — three crates (worktree-service, agent-launch, daemon wiring), a new injected port, remote I/O seam
**Stack**: `#target-parity` 2/7 — branch `feature/target-parity/target-worktree-actions`, base `feature/target-parity/permission-answer`

## Initial Discovery

[2026-11-21-target-worktree-actions-initial-discovery.md](./2026-11-21-target-worktree-actions-initial-discovery.md)

## Affected Packages

- **tddy-worktree-service**: the target-shell port; the target session of a `worktree_path`; Clean / Remove / Restore routed to the target
- **tddy-agent-launch**: the port's implementation over `shell_for_session_target`; `remove_target_worktree` logic reused
- **tddy-daemon**: wires the port into `WorktreeServiceImpl` (`runtime/mount_host.rs`)

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-worktree-actions.md](../../ft/web/1-WIP/PRD-2026-11-21-target-worktree-actions.md)

## Summary

The worktree service learns which sessions run on a target and reaches their worktrees through an injected shell port, so the tab's three actions run where the worktree is and never on the daemon host's clone.

## Responsibility

- `TargetShells` port (tddy-worktree-service): the shell of a session's target, resolved as its owner; `WorktreeServiceImpl::with_target_shells`.
- The target session of a request: by `session_id` (Restore) or by `worktree_path` among the caller's sessions of `project_id` with `target` set and `repo_path` equal to it.
- Clean (`git clean -fdx`), Remove (`git worktree remove --force` + `prune`), Restore (re-cut on the target from the recorded base, `setup_remote_worktree`) through that shell; Restore never rewrites the session's recorded worktree.
- The agent-launch implementation and the daemon wiring.

## Boundaries

- Does not list target worktrees in `StreamWorktreeStats` or size them (`target-worktree-stats`, n7).
- No proto or web change.
- Local sessions' actions unchanged.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` permission-answer | nothing consumed — the line puts it below | — | touch hooks or the follower |

## Draft PR contract

The first push publishes `TargetShells`, `WorktreeServiceImpl::with_target_shells` and the agent-launch implementation type as `todo!()`-bodied surface, plus the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — the tests inject a stand-in shell through the port
**Concurrent with:** n1, n3, n4, n5
**Blocks:** n7 `target-worktree-stats` (measures target rows through this port)

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7):

    n1 permission-answer → n6 question-answer
    n2 target-worktree-actions → n7 target-worktree-stats

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7. n1 and n2 lead wave 1 (one dependent each; n1 first as
the more foundational); n3–n5 block nothing.

## Prerequisites

### ⚠ DURING — [oversized-file-service.md](../../../packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md), [oversized-file-worktrees.md](../../../packages/tddy-worktree-service/docs/code-issues/oversized-file-worktrees.md)

Both files are over budget and unclaimed. The target routing goes in a new module; `service.rs` gains only the dispatch lines.

## Scope

- [ ] Port, builder and the target-session lookup
- [ ] Clean / Remove / Restore on the target
- [ ] Port implementation and daemon wiring
- [ ] Feature doc `session-worktree-inspector.md` updated through the wrap

## Technical Changes

### State A (Current)

Every action resolves against the project's local clone (`resolve_main_repo`): Clean/Remove of a target path are `NotFound`; Restore calls `setup_worktree_for_session_with_integration_base` on the local clone, which rewrites `changeset.worktree`/`repo_path` (`tddy-session-worktree/src/worktree.rs:58-60`).

### State B (Target)

A target session's actions run through its shell on the target; the daemon host is untouched; an unreachable target is reported with its reason.

## Acceptance Tests

`packages/tddy-worktree-service/tests/worktrees_rpc.rs` / `target_worktree_actions_acceptance.rs` (stand-in target shell through the port, real git on both sides):
- [ ] `restore_of_a_target_sessions_worktree_never_cuts_one_on_the_daemons_host` (reproduction, exists)
- [ ] `restore_of_a_target_sessions_worktree_recreates_it_on_the_target`
- [ ] `clean_of_a_target_sessions_worktree_cleans_it_on_the_target`
- [ ] `delete_of_a_target_sessions_worktree_removes_it_on_the_target`
- [ ] `an_action_on_an_unreachable_target_fails_with_its_reason_and_changes_nothing_locally`

`packages/tddy-agent-launch` (unit): the port resolves an ssh target's shell, and refuses a Coder target with a locked vault as `FAILED_PRECONDITION`.

## Decisions & Trade-offs

- **A port, not a dependency**: agent-launch already depends on worktree-service.
- **No proto change**: the target session is found from what each request already carries.

## TODO

- [x] Record initial discovery (`2026-11-21-target-worktree-actions-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
