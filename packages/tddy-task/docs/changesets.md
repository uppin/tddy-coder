# Changesets Applied

Wrapped changeset history for tddy-task.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-25** [Feature] **TaskRegistry broadcast events for real-time subscriptions** — `TaskRegistryEvent` enum (Added/Updated/Removed); `task_events: broadcast::Sender` (cap 256) on `TaskRegistry`; `list_and_subscribe()` (subscribe before snapshot — no events lost); `subscribe_list()` raw receiver; emit points: `spawn`/`register`/`register_terminal` → Added; status change → Updated; `remove()`/eviction → Removed. 5 new registry unit tests. Feature [tasks-ui-realtime.md](../../../docs/ft/web/tasks-ui-realtime.md). (tddy-task)
- **2026-06-24** [Feature] **Long-running background Tasks — new `tddy-task` crate** — `TaskId`, `TaskStatus` (Pending/Running/Completed/Failed/Cancelled), `TaskChannel` (broadcast output + capture ring buffer + optional stdin mpsc), `TaskHandle` (status watch, `CancellationToken`, pid slot, result_json), `TaskContext` (body API: cancel_token, channel writer, register_child_pid, set_result), `TaskBody` trait; `TaskRegistry` (`spawn`, `register`, `get`, `list`, `cancel_task`, `create_terminal_task`, exit-monitor with 5-min TTL + 200-task cap, SIGTERM→SIGKILL escalation safety net). Tests: 5 registry unit tests, 3 cancellation integration tests (real child processes + `libc::kill` liveness probe). Feature [daemon/background-tasks.md](../../../docs/ft/daemon/background-tasks.md). (tddy-task)
