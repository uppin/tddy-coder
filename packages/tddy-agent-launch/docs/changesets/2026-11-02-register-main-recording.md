# 2026-11-02 — Launches register the main terminal's recording

**Type:** Feature

The claude-cli start (`spawn_claude_cli_session_inner`) and resume register the session's `main`
recording through `claude_cli_spawn::record_main_terminal`; every jailed start and relaunch calls
`LaunchSessions::record_jailed_main` (`jail_launch_steps.rs`) right after inserting the sandbox
state. Code issues: `resume_claude_cli_session` 141 → 142 lines and
`start_sandboxed_cursor_cli_session` 432 → 433 (one call each, no branch). See
[module-layout.md](../module-layout.md). `#session-tabs` 2/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-agent-history.md`.
