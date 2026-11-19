# 2026-11-19 — A target agent's pid file is its session's own, and its client's exit is read as the agent's

**Type:** Fix

`remote_agent_pid_file(session_id)` (`.tddy/agents/<session id>.pid`) replaces the
`REMOTE_AGENT_PID_FILE` constant: `remote_agent_command`, `wait_for_agent_start` and
`record_remote_owner` take the session id, and `launch_on_target` clears only that file
(`clear_the_agent_pid_file`), so agents sharing a worktree keep their own pids. The local PTY spawns
through `spawn_rendered_client_main` with the transport's `InteractiveExit` as a
`tddy_pty::ExitCodeInOutput`. See [module-layout.md](../module-layout.md). Full record: [`docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md`](../../../../docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md).
