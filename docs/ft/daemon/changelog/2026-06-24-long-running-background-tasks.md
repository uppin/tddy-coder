# 2026-06-24 — Long-running background Tasks

- New `tasks.TaskService` gRPC service: `ListTasks`, `GetTask`, `WatchTask` (replay-then-live stream with `is_replay` flag), `CancelTask`, `SendInput`
- Every `ExecuteTool` invocation (fast Read/Write/etc.) registers a `Task` in the shared `TaskRegistry`; background Shell tasks observable via `WatchTask`; `Await` tool blocks on `TaskRegistry`
- VM image builds (Buildroot) are now cancellable tasks: `CancelTask` sends SIGINT to the `make` PID; build failure maps to `TaskStatus::Failed` (not `Completed`)
- Cooperative cancellation via `tokio_util::CancellationToken` per task; SIGTERM→SIGKILL escalation safety net after 5 s grace period
- Terminal tasks retained for 5 minutes then evicted; registry capped at 200 terminal tasks (oldest-first eviction)
- Minimal `/tasks` web page: 3-second polling, colour-coded status, Cancel button
- Feature: [daemon/background-tasks.md](../background-tasks.md). Cross-package: [docs/dev/changesets/](../../../dev/changesets/).
