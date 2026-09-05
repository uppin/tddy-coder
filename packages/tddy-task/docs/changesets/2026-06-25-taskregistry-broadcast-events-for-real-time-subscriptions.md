# 2026-06-25 — TaskRegistry broadcast events for real-time subscriptions

**Type:** Feature

`TaskRegistryEvent` enum (Added/Updated/Removed); `task_events: broadcast::Sender` (cap 256) on `TaskRegistry`; `list_and_subscribe()` (subscribe before snapshot — no events lost); `subscribe_list()` raw receiver; emit points: `spawn`/`register`/`register_terminal` → Added; status change → Updated; `remove()`/eviction → Removed. 5 new registry unit tests. Feature [tasks-ui-realtime.md](../../../../docs/ft/web/tasks-ui-realtime.md). (tddy-task)
