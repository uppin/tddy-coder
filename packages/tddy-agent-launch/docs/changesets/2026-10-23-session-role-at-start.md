# 2026-10-23 — A session role recorded before the spawn, granted only by the daemon
**Type:** Feature

`start_session_core` refuses a requested `session_role` (`PERMISSION_DENIED`); `EnsureProjectHomeSession` starts through `start_session_with_role`. The claude-cli start writes the role before spawning (`record_role_before_spawn`) and gives a `project-home` agent one `--allowedTools` per project-home tool, at start and at resume (`role_allowed_tools_args`). See [module-layout.md](../module-layout.md) (#700, `#project-home` 5/7). Code issue `complexity-svc-resume-claude-cli-session-resume-claude-cli-session`: 142 → 144 (row added).
