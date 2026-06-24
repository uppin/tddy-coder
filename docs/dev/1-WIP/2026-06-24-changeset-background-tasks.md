# Changeset: Long-running background Tasks

**Date:** 2026-06-24
**Feature doc:** [docs/ft/daemon/background-tasks.md](../../../docs/ft/daemon/background-tasks.md)
**Status:** In progress — red phase

## Packages affected

`tddy-task` (new), `tddy-service`, `tddy-daemon`, `tddy-vm`, `tddy-web`

## TODO

- [ ] Create/update PRD documentation (`docs/ft/daemon/background-tasks.md`) ✓
- [ ] Create changeset (`docs/dev/1-WIP/2026-06-24-changeset-background-tasks.md`) ✓
- [ ] New leaf crate `packages/tddy-task`: `TaskId`, `TaskStatus`, `TaskChannel`, `TaskHandle`,
      `TaskContext`, `TaskBody` trait, `TaskRegistry` (with `register`, `get`, `remove`, `list`, `spawn`)
- [ ] `packages/tddy-service/proto/tasks.proto`: `TaskService` (ListTasks, GetTask, WatchTask,
      CancelTask, SendInput)
- [ ] Wire `tasks.proto` into `packages/tddy-service/build.rs` (codegen block + descriptor-set entry)
      and re-export `TaskServiceServer` from `packages/tddy-service/src/lib.rs`
- [ ] Add `tddy-task` to root `Cargo.toml` workspace members and to `packages/tddy-daemon/Cargo.toml`
      and `packages/tddy-vm/Cargo.toml`
- [ ] `packages/tddy-daemon/src/task_service.rs`: `TaskServiceImpl` implementing `TaskService` trait
- [ ] Register `tasks.TaskService` in `packages/tddy-daemon/src/main.rs`; inject shared `TaskRegistry`
      into both `ConnectionServiceImpl` and `VmServiceImpl`
- [ ] Delete `packages/tddy-daemon/src/shell_job_registry.rs`; update `tool_engine.rs`,
      `connection_service.rs` to use `TaskRegistry`
- [ ] Refactor `packages/tddy-vm/src/build.rs::build_vm_image_from_spec` to `TaskBody` with
      cancellation via `CancellationToken` + `libc::kill`; update `service.rs::build_vm_image`
- [ ] Retention/cap policy in `TaskRegistry` exit-monitor
- [ ] Minimal `/tasks` page in `packages/tddy-web`
- [ ] Acceptance tests: `packages/tddy-daemon/tests/task_service_acceptance.rs`
- [ ] Crate unit tests: `packages/tddy-task/src/` `#[cfg(test)]`
- [ ] Cancellation integration tests: `packages/tddy-task/tests/cancellation.rs`
- [ ] ExecuteTool fold-in tests: `packages/tddy-daemon/tests/` extensions
- [ ] VM build fold-in tests: extend `packages/tddy-daemon/tests/vm_service_acceptance.rs`

## Validation Results (2026-06-24)

### Build
- `tddy-task`, `tddy-service`, `tddy-daemon`, `tddy-vm`: ✅ `cargo build` clean
- `tddy-web`: ✅ `bun run build` clean (2068 modules)
- Pre-existing failure: `tddy-e2e::grpc_reconnect_second_stream_receives_full_tui_render` — confirmed failing on `main` before our changes, unrelated

### Test suite (our packages)
- `tddy-task`: 6 passed (registry unit tests)
- `tddy-daemon` (task_service_acceptance): 8 passed
- `tddy-daemon` (vm_service_acceptance): 7 passed
- `tddy-vm`: 4 passed
- All 26 tests green ✅

### Issues Found

#### [CRITICAL] `packages/tddy-daemon/src/task_service.rs` — WatchTask can hang
`watch_task` polling loop (`lines 174–215`): after receiving the last live byte, the loop re-checks `handle.status()`. If the tokio scheduler runs `WatchTask` before `set_terminal()` fires in the body task, status is still `Running` and `live_rx.recv()` blocks indefinitely (broadcast sender stays alive in `TaskChannel`/registry).
**Fix needed**: `tokio::select!` on `live_rx.recv()` AND `status_watch().changed()` in the live-stream loop.

#### [CRITICAL] `packages/tddy-vm/src/build.rs` — VmBuildTaskBody always returns `Completed` on error
`build_vm_image_from_spec` returns `()` regardless of outcome; `VmBuildTaskBody::run` (`lines 139–148`) cannot distinguish success from failure and always emits `TaskStatus::Completed { exit_code: Some(0) }` even when STAGE_ERROR was sent.
**Fix needed**: `build_vm_image_from_spec` should return `bool`/`Result`; `run()` maps failure → `TaskStatus::Failed`.

#### [WARNING] `packages/tddy-daemon/src/tool_engine.rs` — `ShellTaskBody` ignores cancel token
`ShellTaskBody::run` (`lines 113–138`) uses `.output()` (waits for process) with no `select!` on `ctx.cancel_token().cancelled()`. Cancel requests go entirely through the 5-second SIGKILL escalation safety net rather than cooperative SIGINT first.
**Recommendation**: Switch to `.spawn()` + `select!` + `ctx.register_child_pid(pid)` for cooperative cancel.

#### [WARNING] `packages/tddy-task/src/registry.rs` — escalation safety-net busy-polls
`escalation_safety_net` (`lines 218–229`) polls `handle.status().is_terminal()` with 100ms sleep intervals instead of using `status_watch().changed()`.
**Recommendation**: replace loop with `tokio::time::timeout(GRACE, wait_terminal(handle))`.

#### [WARNING] `packages/tddy-daemon/src/task_service.rs` — misleading error for zero-channel tasks
Sync tasks (Read/Write/etc.) are created with no channels. `WatchTask` returns `not_found("channel not found")` which implies the task doesn't exist. Should be `failed_precondition("task has no output channels")`.

#### [INFO] Missing tests
- `retention_evicts_terminal_task_after_ttl` (plan item) not yet implemented (TTL requires `tokio::time::pause()`)
- VM build fold-in tests (`build_vm_image_progress_unchanged`, `vm_build_task_is_cancellable`) not yet written

#### [INFO] `packages/tddy-daemon/src/spawner.rs` — unrelated clippy fix included
Cast `gid as libc::c_int` with `#[allow(clippy::cast_possible_wrap)]` is a pre-existing lint fix, unrelated to this changeset. Fine to bundle.

## Acceptance criteria

1. `TaskRegistry.list()` returns all tracked tasks.
2. Every `ExecuteTool` invocation produces a registered task retrievable via `GetTask`.
3. `CancelTask` sends SIGINT to registered child PIDs; escalates to SIGKILL after 5 s if body stalls.
4. `WatchTask` replays capture buffer (`is_replay:true`) then streams live bytes (`is_replay:false`).
5. VM build appears in `ListTasks` and `CancelTask` stops it.
6. Terminal tasks are evicted after 5 min TTL / 200-task cap.
7. `WatchTask` with remote `daemon_instance_id` returns `failed_precondition`.
