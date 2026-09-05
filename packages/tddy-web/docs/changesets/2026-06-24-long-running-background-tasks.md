# 2026-06-24 — **Long-running background Tasks

**Type:** Feature

/tasks web page** — `TasksAppPage.tsx`: 3-second polling via `ListTasks` RPC, colour-coded status badges (Pending/Running/Completed/Failed/Cancelled), Cancel button calls `CancelTask`; `appRoutes.ts`: `TASKS_ROUTE`/`isTasksPath()`; `DaemonNavMenu.tsx`: "Tasks" nav item; `src/gen/tasks_pb.ts` regenerated. Feature [daemon/background-tasks.md](../../../../../docs/ft/daemon/background-tasks.md). (tddy-web)
