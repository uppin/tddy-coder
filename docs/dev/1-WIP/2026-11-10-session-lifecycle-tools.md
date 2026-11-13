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
- [tddy-client-web](../../../packages/tddy-client-web/) and `tddy-rust-typescript-tests` — regenerated `session_pb.ts` (field 37)
- [tddy-tools](../../../packages/tddy-tools/) — closed-world admission list `src/exec_tool_admission.rs` (green adds the five names)
- **`orchestration_mark: None` at every `SessionMetadata` literal** (contract, mechanical — the
  struct has no `Default`, the `tmux_session` precedent #814): tddy-session-activity,
  tddy-session-files, tddy-session-split, tddy-testing-commons, tddy-session-lifecycle,
  tddy-telegram-control, tddy-daemon, tddy-daemon-rpc, tddy-changeset (≈56 sites, mostly tests)

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
| n4 `session-turn-control` (#881) | `ResumeSession` tool, `ProjectHomeTurnPort` pattern | nothing in tests; the mark-clearing sits in the shared resume path (`resume_session_at_session_coordinate`) both reach. The lifecycle module mirrors n4's module/port shape (`ProjectHomeLifecyclePort: ProjectHomePort`, default-refusing methods, `impl … for DaemonProjectHome {}` in `exec_tool/project_home/lifecycle.rs`) | add or change the `ResumeSession` tool or `ProjectHomeTurnPort` |
| n1–n3 (#878–#880) | role, store, effort | nothing — line predecessors. Every test's caller is `project-home` (main), so no `TODO(orchestrator-role)` stub is reached; verified with the `TODO(` grep | touch them |

**Verified (6f, 2026-11-13):** every n5 test fails on a `TODO(session-lifecycle-tools)` stub or on the
missing routing/catalogue wiring this node owns; none reaches an n1–n4 stub.

## Draft PR contract

- `OrchestrationMark` type + metadata field; proto field 37 + gen.
- Five tool defs and `is_project_home_lifecycle_tool`; port methods (`TODO(session-lifecycle-tools)`).
- Drawer badge component (renders nothing yet).
- Failing tests below.

**Published (6d):**

| Symbol | File |
|---|---|
| `OrchestrationMark`, `OrchestrationMarkKind`, `clear_a_paused_mark` (stub: clears nothing) | `packages/tddy-changeset/src/orchestration_mark.rs`, re-exported at the crate root (and `tddy_core::`) |
| `SessionMetadata.orchestration_mark: Option<OrchestrationMark>` | `packages/tddy-changeset/src/session_metadata.rs` |
| `message OrchestrationMark`; `SessionEntry.orchestration_mark = 37` | `packages/tddy-service/proto/session.proto`; TS regenerated in `tddy-client-web/src/gen`, `tddy-rust-typescript-tests/gen` |
| Listing stub `orchestration_mark: None` | `packages/tddy-agent-launch/src/session_coordinate_handlers.rs` (the proto `SessionEntry` mapping) |
| `SET_SESSION_MARK`, `DELETE_SESSION`, `CLEAR_WORKTREE`, `DELETE_WORKTREE`, `RESTORE_WORKTREE`; `is_project_home_lifecycle_tool`; `SessionMark { Mark(kind), None }` + `parse` (stub refuses); `project_home_lifecycle_tool_defs` (`RestoreWorktree { session_id, daemon_instance_id? }`; Clear/Delete `{ path, daemon_instance_id? }`); `ProjectHomeLifecyclePort { set_session_mark, delete_session, clear_worktree, delete_worktree, project_home_worktree, restore_worktree(target) }` (defaults refuse); `execute_project_home_lifecycle_tool` (stub refuses); `NO_LIFECYCLE` | `packages/tddy-tool-engine/src/project_home_lifecycle_tools.rs` |
| `impl ProjectHomeLifecyclePort for DaemonProjectHome {}` | `packages/tddy-daemon-rpc/src/exec_tool/project_home/lifecycle.rs` |
| `SessionDrawerMarkBadge` (renders nothing), rendered in `SessionDrawerItem`; test id `sessionsDrawerItemMark` | `packages/tddy-session-web/src/SessionDrawerMarkBadge.tsx`, `cypress/support/testIds.ts` |

**Not wired (green's):** the five names are in none of `is_project_home_tool`,
`is_project_home_target_tool`, the catalogue (`project_home_tool_defs_with`) or
`DaemonProjectHome::execute`'s dispatch, so a call today falls through `run_locally` to the local
runner — n4's turn tools are left the same way. Green must also extend the **closed-world lists**:
`packages/tddy-tools/src/exec_tool_admission.rs` (~:149), `packages/tddy-tool-engine/src/read_only.rs`
(`mcp__tddy-tools__*` list ~:119), `packages/tddy-daemon-rpc/tests/project_home_session_tools_acceptance.rs`
(~:526), and optionally `tddy-tool-engine/tests/conversation_spawner_scope.rs` (non-spawner tools).

**Contract shape choices (review):** the tool, not the port, stops a paused session — it records
the mark then calls the existing `ProjectHomePort::terminate_session` when the target is active;
the tool builds the `OrchestrationMark` (set_by = caller, at = now); Clear and Delete resolve `path`
against `list_worktrees` and the owner from `target_records`; **`RestoreWorktree` resolves
`session_id`** as the other target tools do (developer, 2026-11-13); **`DeleteWorktree` refuses the
path `ProjectHomeLifecyclePort::project_home_worktree` answers** — the project's home worktree, not the
caller's own (developer, 2026-11-13).

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

Re-verified against `d840a1b96` (2026-11-12/13); moved or added facts marked **(re-verified)**.

- No mark/pause field in `SessionMetadata` (only `status`, `activity_status`, `pending_elicitation`).
  **(re-verified)** It is `deny_unknown_fields` with **no `Default`**: a new field needs a value at
  every struct literal (≈56 sites in 9 crates, mostly tests).
- `DeleteSession` RPC (`session.proto:48`, `tddy-session-activity/src/session_deletion.rs`); no tool.
  **(re-verified)** `delete_session_directory` is at `session_deletion.rs:246` and **terminates** a
  live pid rather than refusing; the RPC is `LaunchSessions::delete_session_at_session_coordinate`
  (`tddy-agent-launch/src/svc_signal_delete_session.rs:59`, teardown `delete_session_as` :90). The
  tool's "refuse a running session" is therefore its own check, not the RPC's.
- `RemoveWorktree`, `CleanWorktree`, `RestoreSessionWorktree` RPCs (`worktree.proto:13,24,25`); no tools.
  **(re-verified)** `remove_worktree_under_repo` `worktrees.rs:889`, `clean_worktree_under_repo`
  `worktrees.rs:954` (both refuse the primary checkout); `restore_session_worktree`
  `tddy-worktree-service/src/service.rs:500` is **addressed by `session_id`, not path** — it re-cuts
  the session's worktree from its changeset.
- `TerminateSession` (`project_home_terminate.rs`, `port.rs:212-223`). **(re-verified)** the trait
  method is `ProjectHomePort::terminate_session` (`tddy-tool-engine/src/project_home_tools.rs:176`);
  the daemon impl is `tddy-daemon-rpc/src/exec_tool/project_home/port.rs:209-222`.
- Git tools' activity gate in `tddy_worktree_service::session_git_ops` **(re-verified)**
  `requires_inactive_target` / `run_git_op` `session_git_ops.rs:105-131`; `is_active` is the target
  record's (`port.rs:276`, pid alive).
- `SessionEntry` last field `reads_host_paths = 36` **(re-verified, `session.proto:370`)**; the proto
  entry is built in `tddy-agent-launch/src/session_coordinate_handlers.rs:~428` from
  `tddy-session-activity/src/session_reader.rs`.
- **(added)** Routing: `ExecToolRpcHandler::run_locally` (`exec_tool/project_home.rs:~100`) serves a
  tool only if `is_project_home_tool` or `is_project_home_git_tool`; forwarding is
  `served_on_target_host` (`project_home_remote.rs:167`) keyed on `is_project_home_target_tool`
  (`project_home_target_host.rs:35`). Resume: `resume_session_at_session_coordinate`
  (`tddy-agent-launch/src/svc_resume_session.rs:34`).

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

### Unit / integration (6f)

- `packages/tddy-tool-engine/tests/project_home_lifecycle_tools.rs` — the tools' rules over a fake
  port: mark words, paused = mark then terminate (only when active), blocked/done/none, refusals
  before the port is asked (unknown mark, other project, running delete, unlisted path, active
  owner, home worktree), the port's refusal passed through, Restore by a recorded path, and all
  five being target tools.
- `packages/tddy-changeset/tests/orchestration_mark.rs` — the mark in `.session.yaml` and
  `clear_a_paused_mark`.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Paused = mark + stop** (developer, 2026-11-10). Blocked/done are marks only.
- **Worktree tools by path**, as `ListWorktrees` lists them, so an orphan worktree with no session can be
  cleaned too — **Clear and Delete only**.
- **`RestoreWorktree` takes `session_id`, not `path`** (developer, 2026-11-13). A deleted worktree is
  no longer listed, and the screen's `RestoreSessionWorktree` is session-addressed; the tool resolves
  the session like the other target tools and refuses an active, unknown or other-project one.
- **The home worktree is the project's home worktree path itself, whoever calls** (developer,
  2026-11-13): `<worktrees root>/<project_id>/home`, the one checkout main and every orchestrator
  share. `DeleteWorktree` refuses it, naming the path; the check never reads the caller's own
  worktree, so it holds for an orchestrator caller exactly as for main.
- **The mark lives in the session record**, not in an orchestrator ledger, so any orchestrator (and the
  web) reads the same state.
- `SessionMetadata` refuses unknown fields: an older daemon cannot read a marked record.

## Refactoring Needed

### From @red (TDD Red Phase)

- Both red-phase plan questions were answered by the developer on 2026-11-13 and are recorded under
  Decisions: `RestoreWorktree` takes `session_id`; the home worktree is the project's home path.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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
