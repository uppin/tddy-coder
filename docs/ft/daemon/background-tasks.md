# Long-running background Tasks

**Product area:** Daemon
**Updated:** 2026-06-24
**Status:** In development — red phase

## Summary

Introduce a first-class, uniform **Task** abstraction to `tddy-daemon`. A Task is any background
operation — from a 2 ms file read to a 60-minute VM image build — tracked in a shared registry with
observable output channels, a cancellation mechanism, and an RPC service for management.

Previously, background work was represented by two unrelated, incomplete mechanisms:
- `ShellJobRegistry` / `ShellJob` (tool engine) — only for the background `Shell` tool, no cancellation, no
  listing, no PID tracking.
- `build_vm_image_from_spec` (tddy-vm) — streaming to an mpsc channel but unstoppable once started.

This feature replaces both with a single, composable task system.

## Relationship to existing systems

### ShellJobRegistry (deprecated)

`packages/tddy-daemon/src/shell_job_registry.rs` and the `ShellJob` struct are **deleted** by this changeset.
The `TaskRegistry` (new crate `packages/tddy-task`) is a strict superset: it adds listing, PID tracking,
cancellation, and 0–N output channels.

`ConnectionServiceImpl.shell_jobs: Arc<ShellJobRegistry>` is replaced by `task_registry: TaskRegistry`.
The `Await` tool continues to work, now reading from `TaskRegistry.get(id).status_watch()` instead of
`ShellJob.done_receiver()`.

### VM build (tddy-vm)

`build_vm_image_from_spec` is refactored to implement `TaskBody` (from `tddy-task`), gaining cooperative
cancellation. `VmService::BuildVmImage` spawns a `vm_build` Task in the shared `TaskRegistry` and then adapts
its `TaskChannel` output to the existing `BuildVmImageProgress` streaming response — so the external API and
existing tests are unchanged, and the build now appears in `TaskService.ListTasks` and is cancellable via
`TaskService.CancelTask`.

## Task model

### TaskId

A `uuid::Uuid::now_v7()` string — consistent with the existing `job_id` convention in `tool_engine.rs:335`.

### TaskStatus lifecycle

```
Pending → Running → Completed { exit_code }
                  → Failed { message }
                  → Cancelled
```

A cancel request while `Running` does not flip status immediately. The task body observes the cancel
signal, performs its own cleanup (internal SIGINT to children), and then returns `Cancelled`.
The registry stores status behind an `Arc<Mutex<TaskStatus>>` and a `watch::Sender<TaskStatus>` so
`WatchTask` and the `Await` tool can await terminal transitions without polling.

### TaskChannel (0–N per task)

Each task declares zero or more named output channels. A channel:
- Emits bytes over a `broadcast::Sender<Bytes>` for multi-subscriber live streaming.
- Maintains a bounded replay ring buffer (64 KB max, mirroring the `CAPTURE_LIMIT_BYTES` constant in
  `claude_cli_session.rs:27`) so late subscribers receive already-emitted output.
- Optionally accepts stdin via an `mpsc::UnboundedSender<Bytes>`.

Fast synchronous tools (Read/Write/Grep…) declare 0 channels; their result lands in
`TaskHandle.result_json`. The background-Shell tool declares 1 combined channel (stdout+stderr).

### Cancellation (per-task CancellationToken)

Each `TaskHandle` holds a `tokio_util::sync::CancellationToken` (new dependency, approved).
Task bodies `select!` on `ctx.cancel_token().cancelled()` at each subprocess wait point and perform
their own cleanup:

1. Task body sends `SIGINT` to each registered child PID via `libc::kill` (same pattern as
   `SignalSession` at `connection_service.rs:1311-1325`).
2. Body awaits child exit with a grace period, then returns `TaskStatus::Cancelled`.
3. Safety-net (registry-level, fires if body does not terminate within ~5 s):
   `SIGTERM → wait → SIGKILL` (mirroring `session_deletion.rs:66-90`).

Child PIDs are tracked per-task in `TaskHandle.pid_slot: Arc<Mutex<Vec<u32>>>`, replacing the
insufficient single global slot `CHILD_PID: AtomicU32` at `tddy-core/src/backend/mod.rs:134`.

### Retention / cap policy

Because every `ExecuteTool` invocation becomes a task (including instant ones), the registry must be
bounded:
- Terminal tasks (Completed / Failed / Cancelled) are retained for a 5-minute TTL so `ListTasks` /
  `GetTask` still report the final status and replay buffer, then removed automatically.
- Total terminal tasks are capped at 200 (evict oldest-first).
- An exit-monitor task drives both the TTL timer and the cap eviction.

## RPC surface

New `tasks.TaskService` (5 methods):

| Method | Shape | Purpose |
|--------|-------|---------|
| `ListTasks` | Unary | Enumerate tasks with status, kind, channels, created timestamp |
| `GetTask` | Unary | Fetch one `TaskInfo` by task ID |
| `WatchTask` | **Server-stream** | Replay buffered channel output then stream live bytes; final event carries terminal status |
| `CancelTask` | Unary | Cooperative cancel (signals the `CancellationToken`; escalates if body stalls) |
| `SendInput` | Unary | Write bytes to a channel's `stdin_tx` (present only on input-capable channels) |

All methods carry `session_token` for auth. Non-list methods scope visibility to the `session_id`
embedded in `TaskHandle`. Each request also carries `daemon_instance_id` for multi-host routing (unary
methods forward via `livekit_peer_discovery::forward_to_peer`; `WatchTask` is local-only this
changeset — returns `failed_precondition` for remote IDs).

### WatchTask replay contract

1. Subscribe to `channel.output_tx` **first** (so no live bytes are missed).
2. Snapshot `channel.capture` and emit all bytes as `TaskOutputEvent { is_replay: true }` chunks.
3. Forward subsequent broadcast messages as `is_replay: false`.
4. When `status_watch` flips terminal, emit a final `TaskOutputEvent { status: <terminal> }` and close
   the stream.

## Web UI

A `/tasks` page in the dashboard, modeled on the `/vms` page (`VmsAppPage.tsx`):
- Table of tasks with status, kind, created timestamp, and Cancel button.
- Clicking a task expands a live-output pane consuming `WatchTask` via `for await`.

Minimal in scope — daemon observability is the focus.

## Architecture

```
packages/tddy-task (new leaf crate)
├── src/lib.rs      — public re-exports
├── src/task.rs     — TaskId, TaskStatus, ChannelKind, TaskChannel, TaskHandle,
│                     TaskContext, TaskBody trait
└── src/registry.rs — TaskRegistry (register, get, remove, list, spawn, retention)

packages/tddy-service
└── proto/tasks.proto  — TaskService definition → Rust TaskServiceServer + TS client types

packages/tddy-daemon
├── src/task_service.rs    — TaskServiceImpl (auth + per-method handlers)
├── src/tool_engine.rs     — every execute_tool call wraps in a Task (via TaskRegistry)
├── src/connection_service.rs — task_registry field replaces shell_jobs
├── src/main.rs            — TaskServiceServer registered + TaskRegistry shared with VmService
└── src/shell_job_registry.rs — DELETED

packages/tddy-vm
├── src/build.rs    — build_vm_image_from_spec refactored to TaskBody (cancellable)
└── src/service.rs  — BuildVmImage spawns a vm_build Task + adapts channel output

packages/tddy-web
└── src/components/tasks/TasksAppPage.tsx  — /tasks page
```

## Requirements

1. `packages/tddy-task` compiles as a leaf crate with no tddy-rpc/tddy-service dependency.
2. `tasks.proto` defines all 5 RPCs; `TaskServiceServer` is registered in `tddy-daemon` and appears in
   gRPC reflection.
3. `TaskRegistry.list()` returns all currently-tracked tasks (running and recently terminal).
4. Every `ExecuteTool` invocation registers a Task; the `task_id` is returned in
   `ExecuteToolResponse.job_id` and resolvable via `GetTask`.
5. Fast-tool Tasks complete near-instantly with 0 channels; the result is stored in `result_json`.
6. Background-`Shell` (block_until_ms=0) Tasks declare a combined channel; the existing `Await` tool
   reads from `TaskRegistry` instead of `ShellJobRegistry`.
7. `CancelTask` sends SIGINT to all registered child PIDs; a 5-second grace then escalates to SIGKILL.
8. `WatchTask` replays the capture buffer before streaming live bytes; `is_replay` distinguishes them.
9. The VM build (`build_vm_image_from_spec`) is cancellable via `CancelTask` and appears in `ListTasks`.
10. Terminal tasks are retained for 5 minutes then evicted; total terminal tasks are capped at 200.

## Out of scope for this changeset

- Streaming multi-host forwarding for `WatchTask` (deferred; returns `failed_precondition` for remote
  `daemon_instance_id`).
- `setpgid` / process-group kill for full subtree cancellation (deferred; current cancellation sends
  SIGINT to the direct child PID only, which may orphan grandchildren of `make`).

## Known gaps / pending design (Updated: 2026-06-24)

- **setpgid subtree kill**: Cancelling a VM build mid-`make` sends SIGINT only to the `make` PID.
  Compiler subprocesses spawned by `make` will not receive the signal and may continue consuming
  resources until they complete. Follow-up: add `setpgid(0, 0)` in a `pre_exec` closure on the `make`
  `Command`, then send `SIGINT` to the negative PGID (`libc::kill(-pgid, SIGINT)`).
- **WatchTask streaming across daemons**: `forward_to_peer` in `livekit_peer_discovery` is unary-only.
  WatchTask on a remote task requires a streaming-forward protocol extension.
