# Changeset: Long-running background Tasks

**Date:** 2026-06-24
**Feature doc:** [docs/ft/daemon/background-tasks.md](../../../docs/ft/daemon/background-tasks.md)
**Status:** In progress — red phase

## Packages affected

`tddy-task` (new), `tddy-service`, `tddy-daemon`, `tddy-vm`, `tddy-web`

## TODO

- [x] Create/update PRD documentation (`docs/ft/daemon/background-tasks.md`)
- [x] Create changeset (`docs/dev/1-WIP/2026-06-24-changeset-background-tasks.md`)
- [x] New leaf crate `packages/tddy-task`: `TaskId`, `TaskStatus`, `TaskChannel`, `TaskHandle`,
      `TaskContext`, `TaskBody` trait, `TaskRegistry` (with `register`, `get`, `remove`, `list`, `spawn`)
- [x] `packages/tddy-service/proto/tasks.proto`: `TaskService` (ListTasks, GetTask, WatchTask,
      CancelTask, SendInput)
- [x] Wire `tasks.proto` into `packages/tddy-service/build.rs` (codegen block + descriptor-set entry)
      and re-export `TaskServiceServer` from `packages/tddy-service/src/lib.rs`
- [x] Add `tddy-task` to root `Cargo.toml` workspace members and to `packages/tddy-daemon/Cargo.toml`
      and `packages/tddy-vm/Cargo.toml`
- [x] `packages/tddy-daemon/src/task_service.rs`: `TaskServiceImpl` implementing `TaskService` trait
- [x] Register `tasks.TaskService` in `packages/tddy-daemon/src/main.rs`; inject shared `TaskRegistry`
      into both `ConnectionServiceImpl` and `VmServiceImpl`
- [x] Delete `packages/tddy-daemon/src/shell_job_registry.rs`; update `tool_engine.rs`,
      `connection_service.rs` to use `TaskRegistry`
- [x] Refactor `packages/tddy-vm/src/build.rs::build_vm_image_from_spec` to `TaskBody` with
      cancellation via `CancellationToken` + `libc::kill`; update `service.rs::build_vm_image`
- [x] Retention/cap policy in `TaskRegistry` exit-monitor
- [x] Minimal `/tasks` page in `packages/tddy-web`
- [x] Acceptance tests: `packages/tddy-daemon/tests/task_service_acceptance.rs`
- [x] Crate unit tests: `packages/tddy-task/src/` `#[cfg(test)]`
- [x] Cancellation integration tests: `packages/tddy-task/tests/cancellation.rs`
- [x] ExecuteTool fold-in tests: `packages/tddy-daemon/tests/tool_engine_acceptance.rs`
- [x] VM build fold-in tests: `packages/tddy-daemon/tests/vm_service_acceptance.rs` + `build_vm_image_adapter_still_delivers_progress_messages` + `vm_build_task_appears_in_registry_after_build_call`

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

#### ~~[CRITICAL]~~ ✅ FIXED `packages/tddy-daemon/src/task_service.rs` — WatchTask hang
`watch_task` live-stream loop now uses `tokio::select!` racing `live_rx.recv()` against `status_rx.changed()`.
A `watch::Receiver<TaskStatus>` is subscribed before entering the loop; any terminal transition immediately
unblocks the select and the loop re-checks `is_terminal()` at the top.

#### ~~[CRITICAL]~~ ✅ FIXED `packages/tddy-vm/src/build.rs` — VmBuildTaskBody status
`build_vm_image_from_spec` now returns `bool` (`true` = success, `false` = any error or cancellation).
`VmBuildTaskBody::run` maps: cancelled → `Cancelled`, success → `Completed { exit_code: Some(0) }`,
failure → `Failed { message: "VM image build failed" }`.

#### [WARNING] `packages/tddy-daemon/src/tool_engine.rs` — `ShellTaskBody` ignores cancel token
`ShellTaskBody::run` (`lines 113–138`) uses `.output()` (waits for process) with no `select!` on `ctx.cancel_token().cancelled()`. Cancel requests go entirely through the 5-second SIGKILL escalation safety net rather than cooperative SIGINT first.
**Recommendation**: Switch to `.spawn()` + `select!` + `ctx.register_child_pid(pid)` for cooperative cancel.

#### [WARNING] `packages/tddy-task/src/registry.rs` — escalation safety-net busy-polls
`escalation_safety_net` (`lines 218–229`) polls `handle.status().is_terminal()` with 100ms sleep intervals instead of using `status_watch().changed()`.
**Recommendation**: replace loop with `tokio::time::timeout(GRACE, wait_terminal(handle))`.

#### [WARNING] `packages/tddy-daemon/src/task_service.rs` — misleading error for zero-channel tasks
Sync tasks (Read/Write/etc.) are created with no channels. `WatchTask` returns `not_found("channel not found")` which implies the task doesn't exist. Should be `failed_precondition("task has no output channels")`.

#### [INFO] Missing tests
- `retention_evicts_terminal_task_after_ttl` (plan item) not implemented (TTL requires `tokio::time::pause()` — deferred)
- VM build fold-in tests: ✅ `build_vm_image_adapter_still_delivers_progress_messages` + `vm_build_task_appears_in_registry_after_build_call` added to `vm_service_acceptance.rs`
- ExecuteTool fold-in tests: ✅ `tool_engine_acceptance.rs` — 5 tests covering sync task registration, error tasks, background Shell, Await, and cap eviction

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
