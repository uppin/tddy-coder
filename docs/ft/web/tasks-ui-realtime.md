# Tasks UI — Real-time View with Channel Output

**Route:** `#/tasks`
**Component:** `TasksDrawerScreen` (`packages/tddy-web/src/components/tasks/`)
**Updated:** 2026-06-25
**Status:** Shipped

## Overview

Upgrade the `/tasks` page from a 3-second-polling table to a real-time two-pane interface.
The left pane lists tasks with live status updates; clicking a task opens the right pane showing
its per-channel stdio output, also streamed in real time.

Backend requirement: a new `WatchTaskList` server-streaming RPC so the frontend receives task
lifecycle events (added/updated/removed) without polling.

The existing `WatchTask` RPC (per-channel streaming, already fully implemented) is used as-is
for the channel output view.

## Layout

```
┌────────────────────┬──────────────────────────────┐
│  TaskDrawer        │  TaskOutputPane               │
│  ─────────────     │                               │
│  ● shell           │  kind: execute_tool:Read      │
│  ○ vm_build        │  status: Completed (exit 0)   │
│  ● execute_tool:.. │  created: 3m ago              │
│                    │  ┌──────────────────────────┐ │
│                    │  │ Tab: stdout  | Tab: stderr│ │
│                    │  │                           │ │
│                    │  │ <scrollable output>       │ │
│                    │  └──────────────────────────┘ │
│                    │  [Cancel] (active tasks only) │
│                    │                               │
│                    │  [nothing selected]           │
│                    │  "Select a task to view output│
└────────────────────┴──────────────────────────────┘
```

## Drawer Items

Each `TaskDrawerItem` shows:
- A **status dot**: blue (`running`), gray (`pending`), green (`completed`), red (`failed`), yellow (`cancelled`)
- **Kind** (truncated with tooltip for full value)
- **Relative time** (e.g. "3m ago")
- **Inline Cancel button** for pending/running tasks

Tasks are ordered newest-first by `createdUnixMs`.

## Real-time List Updates

`useTaskListStream` hook manages the `WatchTaskList` server-streaming subscription:

- Calls `client.watchTaskList()` and iterates the async stream
- Maintains `Map<taskId, TaskInfo>` state
- `task_added` and `task_updated` events → upsert by `taskId` (handles snapshot duplicates)
- `task_removed` events → delete by `taskId`
- `is_snapshot=true` events → batch-apply before re-rendering (or just upsert; idempotent)
- Reconnects on stream end if task is not terminal

## Channel Output View

`TaskOutputPane` shows the selected task's details and channel output:
- **Metadata header**: kind, status (with color), exit code (if completed), created time
- **Cancel button**: shown for pending/running tasks
- **Channel tabs**: one tab per `TaskChannelInfo` in `task.channels`
- Each tab contains `TaskChannelOutput` which subscribes to `WatchTask` for that `(taskId, channelId)`

`useTaskChannelStream(taskId, channelId)` hook:
- Calls `client.watchTask({ taskId, channelId })` and iterates the stream
- Accumulates bytes into a `Uint8Array`
- Decodes as UTF-8 for rendering (lossy, for display)
- Distinguishes replay vs live bytes but renders all as plain text

`TaskChannelOutput` component:
- Scrollable monospace text area
- Auto-scrolls to bottom on new data (unless user has scrolled up)

## WatchTaskList RPC

New server-streaming endpoint added to `tasks.TaskService`:

```protobuf
rpc WatchTaskList (WatchTaskListRequest) returns (stream TaskListEvent);

message WatchTaskListRequest {
  string session_token = 1;
  string daemon_instance_id = 2;
}

message TaskListEvent {
  bool is_snapshot = 1;
  oneof event {
    TaskInfo task_added   = 2;
    TaskInfo task_updated = 3;
    string   task_removed = 4;   // task_id
  }
}
```

Subscribe semantics:
1. Subscribe to live events (broadcast channel)
2. Replay current registry snapshot as `task_added(is_snapshot=true)` events
3. Stream live `TaskRegistryEvent` → `TaskListEvent` until client disconnects

`TaskRegistry` adds `task_events: broadcast::Sender<TaskRegistryEvent>` with capacity 256.
Emit points: `spawn()`, `register()`, `register_terminal()` → `Added`; status change → `Updated`; `remove()` + eviction → `Removed`.

## Route Update

`/tasks` renders `TasksDrawerScreen` instead of the retired `TasksAppPage` (polling-based, deleted).

## RPCs Used

- `WatchTaskList` (new) — real-time task list via server-streaming
- `WatchTask` (existing) — per-channel output streaming
- `CancelTask` (existing) — cancel pending/running tasks

## Known Limitations

- `daemon_instance_id` forwarding is not supported for `WatchTaskList` (same as `WatchTask`).
  Remote daemons return `FAILED_PRECONDITION`.
- No stdin input UI for `accepts_input` channels (deferred).
- `useTaskListStream` does not reconnect after the server closes the stream (100ms idle timeout);
  the task list becomes static until the user reloads. Follow-up: add reconnect loop.
- `useTaskChannelStream` reuses a single `TextDecoder` across channel switches; partial
  multi-byte state is not reset. Unlikely to manifest in practice. Follow-up: reset on switch.
