# 2026-06-25 — Create new session from sessions drawer

**Type:** Feature

`SessionsDrawerScreen` gains `mode: "list" | "creating"` state; `SessionDrawer` gets `+ New session` button (`onCreateSession?` prop); `SessionMainPane` renders `CreateSessionPane` when `isCreating`; new `CreateSessionPane` (tool/claude-cli toggle, project/agent/recipe/model/permission-mode/initial-prompt/branch-intent fields, `ListProjects`+`ListTools`+`ListAgents`+`ListProjectBranches` RPC calls, `StartSession` submit, auto-attach via `ConnectSession` on success); `interceptStartSession` uses middleware pattern for multi-handler intercept safety. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-web)
