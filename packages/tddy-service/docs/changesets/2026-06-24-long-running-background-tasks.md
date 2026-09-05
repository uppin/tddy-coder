# 2026-06-24 — **Long-running background Tasks

**Type:** Feature

tasks.proto** — `tasks.proto`: `TaskService` with `ListTasks`, `GetTask`, `WatchTask` (server-streaming), `CancelTask`, `SendInput`; `TaskInfo`, `TaskChannelInfo`, `TaskOutputEvent` messages; `TaskStatus` enum (PENDING/RUNNING/COMPLETED/FAILED/CANCELLED); `ChannelKind` enum; wired into `build.rs` codegen + descriptor set; `TaskServiceServer` re-exported from `lib.rs`. Feature [daemon/background-tasks.md](../../../../docs/ft/daemon/background-tasks.md). (tddy-service)
