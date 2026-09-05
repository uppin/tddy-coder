# 2026-06-24 — **Long-running background Tasks

**Type:** Feature

daemon wiring** — new `TaskServiceImpl` (5 RPCs: ListTasks, GetTask, WatchTask with subscribe-first + replay-then-live + `tokio::select!` terminal guard, CancelTask, SendInput); shared `TaskRegistry` across `ConnectionServiceImpl`, `VmServiceImpl`, `TaskServiceImpl` via register-then-expose pattern in `main.rs`; `tool_engine.rs`: `ShellTaskBody` (background Shell as task), `register_sync_task` (sync tools as terminal tasks), `Await` reads `TaskRegistry`; `shell_job_registry.rs` deleted; `tasks.TaskService` registered in `main.rs`. Tests: 11 task_service acceptance, 5 tool_engine acceptance (every-action-is-a-task invariant). Feature [daemon/background-tasks.md](../../../../docs/ft/daemon/background-tasks.md). (tddy-daemon)
