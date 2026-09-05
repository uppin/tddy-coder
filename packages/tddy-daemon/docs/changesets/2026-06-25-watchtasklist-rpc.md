# 2026-06-25 — **WatchTaskList RPC

**Type:** Feature

real-time task list streaming** — `TaskServiceImpl.watch_task_list`: authenticates, rejects remote `daemon_instance_id`; `registry.list_and_subscribe()` → snapshot as `is_snapshot=true` events → live `TaskRegistryEvent`→`TaskListEvent` (Added/Updated/Removed) until 100ms idle; `registry_event_to_list_event` helper extracted. 5 acceptance tests. Feature [tasks-ui-realtime.md](../../../../docs/ft/web/tasks-ui-realtime.md). (tddy-daemon)
