# Changeset: Cancel a session start in progress

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto change, several crates (task, daemon-sandbox task service, agent-launch, session-split, spawn, web), cancellation ordering across a forwarded stream and two clone backends, cleanup of partial state
**Stack**: #start-progress 8/8. Base: `feature/start-progress/progress-ui`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-start-cancel-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-09-15-rpcservice-has-no-cancellation-surface.md](../todo/2026-09-15-rpcservice-has-no-cancellation-surface.md) | ⚠ DURING | Not fixed. Cancel is explicit (a task + `CancelTask`), the warm-index precedent the entry names; blocking work observes a `CancellationToken` and kills its child processes. |
| [2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md) | — | Unchanged. |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ DURING | Token checks go into the step helpers (clone, worktree), not as new early returns in the 373-line function. |
| `packages/tddy-session-split/docs/code-issues/complexity-split-claude-cli-start-start-split-claude-cli-session.md` | ⚠ DURING | Peer-cancel goes in node 4's `split_forward_stream.rs` helper. |

## Responsibility

Cancelling a start end to end. Owns:

- `session.proto`: `StartSessionEvent.started = 6` → `StartTaskStarted { task_id, daemon_instance_id }`,
  always the **first** event of a streamed start.
- The streamed start registered in the daemon's `TaskRegistry` as a task whose body is the start; the
  task's `CancellationToken` reaches the clone (stops its `CloneHandle` — worker and supervised
  backends), the worktree cut (kills its git child), and the agent spawn (not started once cancelled).
- Cleanup on cancel: remove the session directory, the worktree (if cut), and the clone's
  `.partial-*` directory; the stream ends with `Status::cancelled("start cancelled")`.
- Split starts: the codebase host's `started` event is relayed with its own task id; cancelling the
  agent host's task sends `CancelTask` for that id to the codebase host, then tears down as today.
- `TaskService.CancelTask` with a `daemon_instance_id` naming a peer is forwarded to that peer over the
  common room (today the field is ignored).
- Web: the create pane records the task id from `started`; while submitting, **Cancel** calls
  `CancelTask` (instead of closing), shows "Cancelling…", and on the cancelled stream returns to the
  editable form showing "Start cancelled" (no error banner).

## Boundaries

- Does not add a generic RPC cancellation surface, or cancel unary `StartSession`, CreateProject or
  AddProjectToHost.
- Does not change progress rendering (node 7) beyond the cancelled state.
- Does not cancel a finished start (Delete exists).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/start-progress/clone-progress` (5/8) | `CloneHandle::stop`, temp-dir clone, `STEP_CLONE` emission | the start task stops the clone through it | change the clone runner or its events |
| `feature/start-progress/supervised-clone-progress` (6/8) | supervised `CloneHandle` (pid from `CloneStarted`) | same, on a supervised host | change the supervisor RPC |
| `feature/start-progress/split-stream` (4/8) | `forward_start_session_relaying`, `split_forward_stream.rs`, `SplitStartFailure` | relays the peer's `started`, sends peer `CancelTask` | change the relay or classification |
| `feature/start-progress/progress-ui` (7/8) | `CreateSessionStartProgress`, `startProgress` state | shows the cancelled state | change step rendering |
| `feature/start-progress/worker-mux` (2/8) | `WorkerEvent::Started { pid }` | a worker clone's handle | change the worker |

## Draft PR contract

```proto
// session.proto
message StartTaskStarted { string task_id = 1; string daemon_instance_id = 2; }
// StartSessionEvent: StartTaskStarted started = 6;   — always first on a streamed start
```

```rust
// tddy-agent-launch: the start as a task
pub(crate) struct StartTask { /* token, registry id, cleanup */ }
pub fn start_task_body(…) -> tddy_task::TaskBody;     // registered by stream_start_session_at_session_coordinate
// tddy-daemon-sandbox/src/task_service.rs — cancel_task forwards when daemon_instance_id names a peer
// tddy-daemon-kernel/src/peer_calls.rs
pub async fn forward_cancel_task(room: &CommonRoom, peer: &str, req: &CancelTaskRequest) -> Result<CancelTaskResponse, Status>;
```

```ts
// tddy-web: useCreateSessionSubmit → cancelStart(): Promise<void>; startProgress gains `cancelled: boolean`
// CreateSessionActions: Cancel enabled while submitting when a task id is known
```

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — needs nodes 5 and 6 (clone handles on both backends) and node 4
(the relayed split stream) green.
**Concurrent with:** none
**Blocks:** none (the sweep node follows)

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-service**: `session.proto` (+ generated Rust and `tddy-web/src/gen/session_pb.ts`)
- **tddy-agent-launch**: start task, token checks, cleanup
- **tddy-session-split**: peer cancel on split starts
- **tddy-daemon-kernel**: `forward_cancel_task`
- **tddy-daemon-sandbox**: `task_service.rs` remote forwarding
- **tddy-web**: Cancel during a start

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-start-cancel.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress-start-cancel.md)
- [background-tasks.md](../../ft/daemon/background-tasks.md), [session-drawer.md](../../ft/web/session-drawer.md)

## Summary

A streamed start is a task; cancelling it stops the clone and worktree work on every host involved,
removes partial state, and returns the create pane to the form.

## Background

Nothing can stop a start today: the web transport ignores `AbortSignal` on server streams, the server
start runs on a detached task whose sink ignores a gone caller, `spawn_blocking` work cannot be
aborted, and `CancelTask` ignores its `daemon_instance_id`.

## Scope

- [ ] `started` event first on every streamed start
- [ ] Start registered as a task; token reaches clone, worktree, agent spawn
- [ ] Cleanup on cancel; stream ends `CANCELLED`
- [ ] Split: relay peer task id; cancel the peer's task
- [ ] `CancelTask` forwarded to a peer daemon
- [ ] Web: Cancel during a start, "Cancelling…", "Start cancelled"
- [ ] Docs: background-tasks (start task), session-drawer (Cancel), remote-managed-worktree (cancel)

## Technical Changes

### State A
`stream_start_session_at_session_coordinate` `tokio::spawn`s the start; no task, no token; `CancelTask`
local only; the pane's Cancel is disabled while submitting.

### State B
The handler registers a `StartTask`, sends `started{task_id}` first, runs the start in the task body with
its token; a cancelled token stops the clone handle / kills worktree git / skips the agent, runs cleanup,
and ends the stream `CANCELLED`. Split: the peer's `started` id is kept; cancel forwards `CancelTask` to
it. `CancelTask{daemon_instance_id: peer}` is forwarded.

## Testing Plan

Session-level tests (`start_phase_acceptance.rs` style) cancelling during a slow clone (fake git) and
during a worktree cut; split test with a scripted codebase host asserting it receives `CancelTask`;
task-service test for peer forwarding with a scripted forwarder; Cypress component test for the pane.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_streamed_start_names_its_task_first` | `packages/tddy-session-lifecycle/tests/start_cancel_acceptance.rs` (new) | `started` first |
| `cancelling_a_start_that_is_cloning_kills_the_clone_and_leaves_no_repository` | same | clone stop + cleanup |
| `cancelling_during_the_worktree_cut_leaves_no_session_directory_or_worktree` | same | worktree cleanup |
| `a_cancelled_start_ends_its_stream_cancelled` | same | terminal status |
| `cancelling_a_split_start_cancels_the_codebase_hosts_task` | `packages/tddy-session-split/tests/split_room_hosting_port.rs` | peer `CancelTask` |
| `cancel_task_for_a_peer_daemon_is_forwarded_to_it` | `packages/tddy-daemon-sandbox/tests/task_service_peer_cancel.rs` (new) | forwarding |
| `Cancel during a start cancels it and returns to the form saying so` | `packages/tddy-web/cypress/component/CreateSessionStartCancel.cy.tsx` (new) | UI |

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **tddy Task, not `RpcRequest.abort`** (developer's choice): explicit, observable in the Tasks UI,
  and independent of transport cancellation, which does not exist end to end.
- **Complexity 5 accepted**: cancel is one capability across hops; splitting UI from daemon would leave
  a daemon cancel nothing calls, or a button nothing honours.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-start-progress-start-cancel-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests (`./test -p tddy-agent-launch -p tddy-session-split -p tddy-daemon-kernel -p tddy-daemon-sandbox -p tddy-session-lifecycle`) + the changed Cypress spec; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped clippy, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
