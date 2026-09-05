# 2026-06-25 — Create new session from sessions drawer

**Type:** Feature

`tddy-web`: `SessionsDrawerScreen` gains `mode: "list" | "creating"` state; `SessionDrawer` gets `+ New session` button; `SessionMainPane` renders `CreateSessionPane` when creating; new `CreateSessionPane` component (tool/claude-cli toggle, project/agent/model/recipe/branch-intent fields, `ListProjects`+`ListTools`+`ListAgents`+`ListProjectBranches` RPC calls, `StartSession` submit, auto-attach on success). Feature [session-drawer.md](../ft/web/session-drawer.md). (tddy-web)
