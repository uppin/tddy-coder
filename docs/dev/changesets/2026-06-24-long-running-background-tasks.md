# 2026-06-24 — Long-running background Tasks

**Type:** Feature

new `tddy-task` leaf crate (TaskId/TaskStatus/TaskChannel/TaskHandle/TaskBody/TaskRegistry + cooperative cancel + SIGKILL escalation + TTL/cap retention); `tasks.proto` (`TaskService` 5 RPCs: List/Get/WatchTask/Cancel/SendInput); daemon `TaskServiceImpl` + shared registry across Connection/Vm/TaskService; every `ExecuteTool` invocation becomes a task; VM build refactored to `VmBuildTaskBody` (cancellable, status-correct); minimal `/tasks` web page with polling + cancel. Feature [daemon/background-tasks.md](../ft/daemon/background-tasks.md). (tddy-task, tddy-service, tddy-daemon, tddy-vm, tddy-web)
