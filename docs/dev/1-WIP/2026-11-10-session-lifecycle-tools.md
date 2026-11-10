# Changeset: orchestrators pause, delete and clean up sessions

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Feature
**Complexity**: 4 — more than one crate (changeset, tool-engine, daemon-rpc, worktree-service, session-activity, service proto, session-web), a proto field, destructive operations behind an activity gate
**Stack**: `#orchestrators` 5/11 · branch `feature/orchestrators/session-lifecycle-tools` · base `feature/orchestrators/session-turn-control` (#881)

## Initial Discovery

[2026-11-10-session-lifecycle-tools-initial-discovery.md](./2026-11-10-session-lifecycle-tools-initial-discovery.md)

## Prerequisites

### ✅ RESOLVED HERE — Project-home worktree tools stop at reading: no Clear, Delete or Restore — [`2026-10-25-project-home-worktree-clear-delete-restore.md`](../todo/2026-10-25-project-home-worktree-clear-delete-restore.md)

`ClearWorktree`, `DeleteWorktree`, `RestoreWorktree` over the functions the RPCs call, activity-gated,
forwarded — exactly what the entry says would close it. The developer asked for full session control
(2026-11-10), which answers its "nobody asked" reason.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-changeset](../../../packages/tddy-changeset/README.md) — `SessionMetadata.orchestration_mark`
- [tddy-service](../../../packages/tddy-service/) — `SessionEntry.orchestration_mark` (37)
- [tddy-tool-engine](../../../packages/tddy-tool-engine/README.md) — five tool defs
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — ports, routing, forwarding
- [tddy-worktree-service](../../../packages/tddy-worktree-service/) — reuse of clean/remove/restore
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — resume clears `paused`
- [tddy-session-activity](../../../packages/tddy-session-activity/README.md) — list the mark
- [tddy-session-web](../../../packages/tddy-session-web/docs/README.md) — drawer badge

## Related Feature Documentation

- PRD: [PRD-2026-11-10-session-lifecycle-tools.md](../../ft/daemon/1-WIP/PRD-2026-11-10-session-lifecycle-tools.md)
- [Project concept § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools), [Worktrees](../../ft/web/worktrees.md), [Session drawer](../../ft/web/session-drawer.md)

## Summary

Five orchestrator tools: `SetSessionMark` (paused / blocked / done / none + note; `paused` also stops
the session, any resume clears it), `DeleteSession`, and the Worktrees screen's `ClearWorktree`,
`DeleteWorktree`, `RestoreWorktree`. The mark is listed on `SessionEntry` and shown on the drawer row.

## Background

`/yolo-pr-stack` keeps a per-node ledger (queued, blocked, done, stopped) and removes each worktree when
its node finishes. The developer wants orchestrators to do the same and to "mark them as paused"
(2026-11-10); a paused mark also stops the session (developer answer, 2026-11-10).

## Responsibility

- `OrchestrationMark { mark: Paused | Blocked | Done, note, set_by_session_id, at_unix_ms }` in
  `.session.yaml` and `SessionEntry.orchestration_mark`.
- `SetSessionMark`, `DeleteSession`, `ClearWorktree`, `DeleteWorktree`, `RestoreWorktree` — tool-engine
  module `project_home_lifecycle_tools.rs`, daemon port, catalogue, forwarding.
- Clearing `paused` on resume, in the daemon's resume path (`resume_session_at_session_coordinate`).
- Drawer badge `sessions-drawer-item-mark-<id>` with the note as its tooltip.

## Boundaries

- Does **not** add `SendSessionMessage` / `ResumeSession` (n4). Its resume test drives the
  `ResumeSession` **RPC**, not n4's tool, so this node needs no n4 behaviour.
- Does **not** change the web's Delete or the Worktrees screen.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n4 `session-turn-control` (#881) | `ResumeSession` tool | nothing in tests; the mark-clearing sits in the shared resume path both reach | add or change the `ResumeSession` tool |
| n1–n3 (#878–#880) | role, store, effort | nothing — line predecessors | touch them |

## Draft PR contract

- `OrchestrationMark` type + metadata field; proto field 37 + gen.
- Five tool defs and `is_project_home_lifecycle_tool`; port methods (`TODO(session-lifecycle-tools)`).
- Drawer badge component (renders nothing yet).
- Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — tests use main and the `ResumeSession` RPC
**Concurrent with:** n1, n2, n3, n4, n6
**Blocks:** none

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] Mark model, proto, listing
- [ ] `SetSessionMark`; `paused` stops; resume clears
- [ ] `DeleteSession` (refuses running)
- [ ] ✅ `ClearWorktree`, `DeleteWorktree`, `RestoreWorktree` ([todo](../todo/2026-10-25-project-home-worktree-clear-delete-restore.md))
- [ ] Forwarding for all five
- [ ] Drawer badge

## Technical Changes

### State A (Current)

- No mark/pause field in `SessionMetadata` (only `status`, `activity_status`, `pending_elicitation`).
- `DeleteSession` RPC (`session.proto:48`, `tddy-session-activity/src/session_deletion.rs`); no tool.
- `RemoveWorktree`, `CleanWorktree`, `RestoreSessionWorktree` RPCs (`worktree.proto:13,24,25`); no tools.
- `TerminateSession` (`project_home_terminate.rs`, `port.rs:212-223`).
- Git tools' activity gate in `tddy_worktree_service::session_git_ops`.
- `SessionEntry` last field `reads_host_paths = 36`.

### State B (Target)

- Five tools as above; `paused` = mark + SIGTERM; resume clears; mark listed and badged.

### Delta

- changeset, proto, tool-engine, daemon-rpc, worktree-service (call reuse), agent-launch (resume
  clears), session-activity (listing), session-web (badge).

## Implementation Milestones

- [ ] Mark model + listing
- [ ] SetSessionMark + resume clearing
- [ ] DeleteSession
- [ ] Worktree tools
- [ ] Forwarding
- [ ] Badge

## Testing Plan

**Level**: exec-tool acceptance in `tddy-daemon-rpc` (project-home world, real git worktrees in temp
dirs), Cypress component for the badge.

## Acceptance Tests

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_home_lifecycle_tools_acceptance.rs`
- `marking_a_running_session_paused_stops_it_and_records_the_mark`
- `resuming_a_paused_session_clears_its_mark`
- `a_blocked_or_done_mark_leaves_the_session_running`
- `delete_session_deletes_a_stopped_session_and_refuses_a_running_one`
- `all_five_tools_reach_another_host`

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/worktree_tools_acceptance.rs` (beside the existing cases)
- `delete_worktree_removes_a_clean_idle_worktree_and_keeps_its_branch`
- `delete_worktree_refuses_a_dirty_worktree_naming_commit_and_stash`
- `delete_worktree_refuses_the_home_worktree`
- `a_worktree_tool_is_refused_while_its_session_is_active`
- `clear_worktree_and_restore_worktree_do_what_the_screen_does`

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerMarkBadgeAcceptance.cy.tsx`
- `a marked session shows its mark with the note on hover`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Paused = mark + stop** (developer, 2026-11-10). Blocked/done are marks only.
- **Worktree tools by path**, as `ListWorktrees` lists them, so an orphan worktree with no session can be
  cleaned too.
- **The mark lives in the session record**, not in an orchestrator ledger, so any orchestrator (and the
  web) reads the same state.
- `SessionMetadata` refuses unknown fields: an older daemon cannot read a marked record.

## Refactoring Needed

### From @red (TDD Red Phase)
### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### Change Validation (@validate-changes)
### Test Validation (@validate-tests)
### Production Readiness (@prod-ready)
### Code Quality (@analyze-clean-code)

## Successor PRs

n6 `feature/orchestrators/conversation-rotation`.

## TODO

- [x] Record initial discovery (`2026-11-10-session-lifecycle-tools-initial-discovery.md`)
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
