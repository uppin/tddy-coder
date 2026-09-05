# 2026-06-25 — Tasks UI: real-time two-pane view with WatchTaskList streaming

- `/tasks` upgraded from 3-second polling table to `TasksDrawerScreen`: live two-pane layout (left drawer + right output pane)
- `useTaskListStream` subscribes to new `WatchTaskList` server-streaming RPC; `Map<taskId, TaskInfo>` updated in real time without polling
- `TaskDrawerItem`: status dot (blue/gray/green/red/yellow by status), kind text (truncated), inline Cancel button for pending/running tasks; newest-first order
- `TaskOutputPane`: per-channel tabs (one per `TaskChannelInfo`); `TaskChannelOutput` streams bytes via existing `WatchTask` RPC with auto-scroll
- Feature: [tasks-ui-realtime.md](../tasks-ui-realtime.md)
