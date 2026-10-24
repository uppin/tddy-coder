# 2026-10-24 — A streamed session start is a cancellable task

**Type:** Feature

`#start-progress` 8/8 ([#741](https://github.com/uppin/tddy-coder/pull/741)), the stack's top node; base
[#740](https://github.com/uppin/tddy-coder/pull/740) (progress-ui). Changes, by crate:

- `tddy-service`: `StartSessionEvent.started = 6` → `StartTaskStarted { task_id, daemon_instance_id }`,
  always first on a streamed start. Generated Rust plus `tddy-web/src/gen/session_pb.ts`.
- `tddy-agent-launch`: `start_task.rs` (`StartTask`, a `TaskBody` registered as `start_session`) replaces
  the detached `tokio::spawn` in `stream_start_session_at_session_coordinate`. The task's token reaches
  the clone through `clone_stopper.rs` and `CloneBackend::clone_repo(…, on_started, …)` on all three
  backends. `refuse_a_cancelled_start` runs after the clone. A start that answers after being
  cancelled has its session deleted (`undo_a_cancelled_start` → `delete_session_at_session_coordinate`).
  `start_session_core_cancellable` is the token-taking form of `start_session_core`.
- `tddy-session-split`: the split start races its forward against the token. `forward_split_start`
  relays and remembers the codebase host's `started` (`CodebaseStartTask`). `cancel_codebase_half`
  forwards `CancelTask` to it and tears the workspace session down.
- `tddy-daemon-kernel`: `peer_calls::forward_cancel_task`.
- `tddy-daemon-sandbox`: `TaskServiceImpl::forwarding_to_peers`, which `tddy-daemon`'s
  `mount_entries.rs` wires when the daemon has a common room.
- `tddy-task`: re-exports `CancellationToken`.
- `tddy-web`: Cancel during a start (`useCreateSessionSubmit.startCancel`, `CreateSessionActions`). The
  `taskClient` comes from `useSessionsDrawerClients` and `useSessionsDrawerOwningHost` and is passed
  through `SessionMainPane` to `CreateSessionPane` and to the PR-Stack `CreateSessionDialog`.

**Decisions.** A tddy Task rather than transport cancellation, which does not exist end to end (the
web transport ignores `AbortSignal` on server streams). Cancellation is cooperative, not a dropped
future: the clone removes its own `.partial-*` before the stream ends. A worktree cut already running
is waited out and then undone by the same path as `DeleteSession`. Killing its git child is a
start-path refactor; it is recorded in the backlog as "a cancelled start waits out a worktree cut (or
agent spawn) already running" (`2026-10-24-cancelled-start-waits-out-an-in-flight-worktree-cut`).
The project-home main variant offers no Cancel, because its start is not streamed.

**Tests.** `start_cancel_acceptance.rs` (3, including
`cancelling_during_the_worktree_cut_leaves_no_session_directory_or_worktree`, which holds the cut with a
`post-checkout` hook), `split_room_hosting_port::cancelling_a_split_start_cancels_the_codebase_hosts_task`,
`task_service_peer_cancel.rs` (2), and `CreateSessionStartCancel.cy.tsx`.

**Code issues (measured, deferred with the developer's consent):**

- `start_session_on_request`: 381 → 391 lines.
- `start_split_claude_cli_session`: 153 → 170 lines.
- `CreateSessionPane.tsx`: 502 → 506 lines.
- `SessionsDrawerScreen.tsx`: 497 → 504 lines (new record).
- `PrStackScreen.tsx`: 698 → 704 lines (new record).

`session_coordinate_handlers.rs` shrank from 499 to 460.

The RpcService-has-no-cancellation-surface backlog entry stays open. This change uses the explicit
task cancel that entry names as the precedent.
