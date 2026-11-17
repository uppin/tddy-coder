# 2026-11-17 — The session manager announces agent ends

**Type:** Feature · `#activity-wake` 1/3, PR [#929](https://github.com/uppin/tddy-coder/pull/929)

`cli_session_manager/agent_end.rs`: `AgentEnd { session_id, os_user, outcome, exit_code }` on the manager's broadcast channel, taken with `CliSessionManager::agent_ends()`. Both announcing sites send exactly once — a main classified `AgentEnded` (a PTY-run main with its outcome and exit code; a tmux-run main as `exit code unknown` with no code) and the detach watcher when a detached agent's tmux session is gone. A tab and a tmux client ending while its agent lives announce nothing. Full record: [`docs/dev/changesets/2026-11-17-session-ended-notice.md`](../../../../docs/dev/changesets/2026-11-17-session-ended-notice.md).
