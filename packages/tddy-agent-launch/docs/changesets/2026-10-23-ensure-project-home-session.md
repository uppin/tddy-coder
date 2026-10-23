# 2026-10-23 — EnsureProjectHomeSession
**Type:** Feature

`svc_ensure_project_home.rs`: the project-home session's started / resumed / running decision, its two refusals (no home agent; a harness other than `claude-cli`), forwarding to the named daemon, and the managed start on the home worktree with `session_role: project-home` recorded. A non-sandboxed claude-cli start runs in a non-empty `repo_path` directly (`claude_cli_checkout`) instead of cutting a session worktree. See [module layout](../module-layout.md) and [project concept § Project-home session](../../../../docs/ft/daemon/project-concept.md#project-home-session) (#699, `#project-home` 4/7).
