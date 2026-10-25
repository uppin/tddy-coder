# Changeset: StartSession returns at once; poll or cancel the start

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — concurrency (a background stream consumer), a persisted ledger, multi-daemon cancel; two crates
**PR**: [#753](https://github.com/uppin/tddy-coder/pull/753)
**Stack**: `#start-parity` 4/9 — branch `feature/start-parity/async-start`, base `n3` `worktree-list` (`feature/start-parity/worktree-list`)
**PRD**: [PRD-2026-10-25-start-parity-async-start.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-async-start.md)

## Initial Discovery

See [./2026-10-25-start-parity-async-start-initial-discovery.md](./2026-10-25-start-parity-async-start-initial-discovery.md).

## Prerequisites

- ⚠ [2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md](../todo/2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md) — some start types emit no phases; recorded in the tool description, not fixed here.
- ⚠ [2026-10-24-cancelled-start-waits-out-an-in-flight-worktree-cut.md](../todo/2026-10-24-cancelled-start-waits-out-an-in-flight-worktree-cut.md) — `CancelSessionStart` inherits the delay.
- ⚠ [2026-10-24-streamed-start-keepalive-through-a-slow-worktree-step-untested.md](../todo/2026-10-24-streamed-start-keepalive-through-a-slow-worktree-step-untested.md) — the blocking-hook seam built here is the one that entry asks for. Reference only; not claimed.

## Responsibility

- The ledger: its record type, the stream consumer, and persistence.
- `StartSession` returning a handle.
- `GetSessionStart` and `CancelSessionStart`.

## Boundaries

- Does not change the start core, the task registry or `TaskService`.
- Does not add start phases to starts that emit none: tool, sandboxed and stack-child starts
  (TODO 2026-10-03). For those starts the ledger reads `running` until the outcome, and the tool
  description says so.
- Does not touch `spawn_conversation` (that is `spawn-conversation`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` `start-tool` (`feature/start-parity/start-tool`) | `StartSessionArgs`, the `StartSession` tool, its schema, and the orchestrator fields 44/45 | builds on `StartSessionArgs` and the `StartSession` tool | change the args model, the schema generator, or the proto fields |

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

- `tddy-tool-engine::start_ledger` (moved from the planned `tddy-daemon-rpc/src/exec_tool/start_ledger.rs`).
  It lives in the tool engine because `tddy-agent-launch` (whose spawn path `spawn-conversation`
  puts on this ledger) depends on it, and cannot depend on `tddy-daemon-rpc`.
  - `GET_SESSION_START` and `CANCEL_SESSION_START`;
  - `StartHandle {start_id, task_id, daemon_instance_id}`;
  - `StartState` (`running`, `succeeded`, `failed`, `cancelled`);
  - `StartStep {step, status}` and `StartCloneProgress {stage, percent}`;
  - `StartLedgerRecord {…, session_id, branch, worktree, error, updated_at_unix_ms}`;
  - `StartLedger::{in_session_dir, read, recover_interrupted}`;
  - `begin_recorded_start(ledger, events) -> Result<StartHandle, String>`.

  Every body is `TODO(async-start)`. The record lives at `<session dir>/artifacts/session-starts/`.
- The tool definitions and the daemon wiring (`StartSession` returning a handle, local and
  forwarded cancel) are this node's private implementation.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its tests exercise `n2` `start-tool` (`feature/start-parity/start-tool`)'s behaviour, which must be green first
**Concurrent with:** `n3` `worktree-list` (`feature/start-parity/worktree-list`), `n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`)
**Blocks:** `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`), `n8` `disk-refresh` (`feature/start-parity/disk-refresh`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-async-start.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-async-start.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

`StartSession` blocks until the clone, the worktree and the agent are done, and gives back only a
session id. This node makes it return a handle at once. A ledger records the start's phases
and its outcome, `GetSessionStart` reads it, and `CancelSessionStart` stops it on whichever host it
runs.

## Scope

- [ ] `StartSession` returns before the start finishes, with a handle.
- [ ] The orchestrator can read the start's current phase and its outcome: the session, branch and worktree, or the reason it failed.
- [ ] The orchestrator can cancel a start on any host.
- [ ] The record can still be read after the daemon's task list has dropped the start.

## Technical Changes

### State A

- `StartSession` (the tool from `start-tool`) awaits the unary `StartSession`. That call is not a
  task, sends its progress to a discarding sink, and cannot be cancelled.
- `StreamStartSession` runs as a `start_session` task, and its first event is
  `StartTaskStarted{task_id, daemon_instance_id}`, for a forwarded start too. But phases and clone
  progress live only in the stream, `result_json` is never set, `GetTask` is local only, and a
  terminal task is evicted after 5 minutes (`tddy-task/src/registry.rs:12-14`).
- `CancelTask` forwards to a peer (`tddy-daemon-sandbox/src/task_service.rs:269-296`).

### State B

- `StartSession` opens `StreamStartSession`. It waits for `StartTaskStarted` (or an immediate
  failure) and returns `{start_id, task_id, daemon_instance_id}`.
- A background consumer on the caller's host drains the stream into the **start ledger**:
  - `{start_id, state: running|succeeded|failed|cancelled, phase: {step, status}, clone_progress?,
    session_id?, branch?, worktree?, error?, updated_at}`;
  - persisted at `<home session dir>/artifacts/session-starts/<start_id>.json`, so it outlives the
    task registry's eviction.
- `GetSessionStart {start_id}` reads the ledger record.
- `CancelSessionStart {start_id}` sends `CancelTask{task_id, daemon_instance_id}` and returns at
  once. The ledger then reads `cancelled` once the stream ends with CANCELLED.

## Testing Plan

### Acceptance Tests

- [ ] **Unit** `packages/tddy-tool-engine/tests/start_ledger.rs` (a scripted
  `StreamStartSession` event stream):
  - `the_handle_names_the_starts_task_and_host_before_the_start_ends`
  - `the_record_follows_the_starts_step_and_ends_with_its_session`
  - `a_start_ending_cancelled_reads_cancelled`
  - `a_start_failing_reads_failed_with_its_reason`
  - `a_stream_that_ends_without_a_result_reads_failed_saying_so`
  - `a_start_refused_before_naming_its_task_is_the_callers_error`
  - `the_record_outlives_the_ledger_that_wrote_it`
  - `a_record_left_running_by_a_restart_reads_failed_naming_the_restart`
  - `an_unknown_start_is_refused_naming_it`
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/async_start_acceptance.rs`. The project
  repo's `post-checkout` hook blocks until the test releases it.
  - `start_session_returns_a_handle_while_the_worktree_is_still_being_cut`
  - `once_released_the_start_reads_succeeded_with_its_session_branch_and_worktree`
  - `cancelling_a_held_start_leaves_no_session_and_no_branch`
  - `a_start_the_start_path_refuses_is_the_tools_error_naming_why`
  - `an_unknown_start_is_refused_naming_it`

- [ ] **Integration** same file, host-b's checkouts held too (developer, 2026-10-25):
  - `a_start_on_another_host_is_followed_from_the_orchestrator_until_it_succeeds` — the record
    reports host-b's worktree step, then `succeeded` with `daemon_instance_id = host-b`; the
    session exists on host-b only.
  - `cancelling_a_start_on_another_host_stops_it_there` — `cancelled`, with no session and no
    branch on host-b.

  **This node's green extends the shared two-host world** (`tests/support/mod.rs`, added by #751).
  `InProcessPeers` gains `session.SessionService/StreamStartSession` in `call_server_stream` and
  `tasks.TaskService/CancelTask` in `call_unary`. The change is additive: no existing suite's
  behaviour changes.

**Unproven until green:** that the blocking `post-checkout` hook holds a workspace start inside its
worktree step. It is the seam `docs/dev/todo/2026-10-24-streamed-start-keepalive-through-a-slow-worktree-step-untested.md`
names. If `git worktree add` does not run it for the start path, the hook moves to whatever the
worktree step runs.

Tests are written in the `fluent-tests` style (Given / When / Then). 

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **The ledger sits on the caller's host and is fed by the stream.** The alternative was
  forwarding `GetTask` plus a phase store on `TaskHandle`. A forwarded stream already relays the
  peer's events, so one consumer covers local and remote starts, and nothing in `tddy-task`
  changes.
- **If the consumer outlives the daemon,** a restart leaves a `running` record behind. On load,
  the record is marked `failed: "daemon restarted during the start"` — that is a recorded fact,
  not a fallback.
- **A cancelled start deletes the branch it cut** (developer decision, 2026-10-25). The cancel's
  undo removes the session and then force-deletes, in the project repository on the host that ran
  the start, the branch its changeset records when the changeset resolved to `new_branch_from_base`
  (an empty intent resolves to it). A `work_on_selected_branch` start never deletes its branch, and
  a failure to delete the cut branch is named in the cancel's status. The branch is local only: a
  branch the start pushed to origin (`create_remote_branch`) is not deleted there.

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

- [x] Record initial discovery (`2026-10-25-start-parity-async-start-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
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
