# 2026-06-25 — Create new session from sessions drawer

- `+ New session` button in the `SessionDrawer` header switches `SessionsDrawerScreen` to `"creating"` mode
- `CreateSessionPane` replaces the main pane: tool vs Claude CLI toggle; project (required), agent/recipe or model/permission-mode/initial-prompt fields; branch intent (new branch from base or work on existing branch with `ListProjectBranches` dropdown)
- On submit: `StartSession` RPC; on success: auto-navigate to `/sessions/:newId` and auto-attach via `ConnectSession`; on error: inline error message, form stays open
- Cancel returns to the previous session list / placeholder state
- 29 new Cypress component tests (12 acceptance, 17 unit)
