# 2026-06-24 — **Long-running background Tasks

**Type:** Feature

new `tddy-task` crate** — `TaskId`, `TaskStatus` (Pending/Running/Completed/Failed/Cancelled), `TaskChannel` (broadcast output + capture ring buffer + optional stdin mpsc), `TaskHandle` (status watch, `CancellationToken`, pid slot, result_json), `TaskContext` (body API: cancel_token, channel writer, register_child_pid, set_result), `TaskBody` trait; `TaskRegistry` (`spawn`, `register`, `get`, `list`, `cancel_task`, `create_terminal_task`, exit-monitor with 5-min TTL + 200-task cap, SIGTERM→SIGKILL escalation safety net). Tests: 5 registry unit tests, 3 cancellation integration tests (real child processes + `libc::kill` liveness probe). Feature [daemon/background-tasks.md](../../../../docs/ft/daemon/background-tasks.md). (tddy-task)
