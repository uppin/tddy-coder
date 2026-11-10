# 2026-11-07 — Launches take the session user's agent-tool socket

**Type:** Feature

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

- `LaunchSessions.agent_tool_sockets` and `agent_tool_socket_of(os_user)`: resolved before a start or resume cuts anything, `FAILED_PRECONDITION` naming `agent_tool_sockets` on refusal. `claude_cli_hooks`, `AgentNotifyLaunch`, `resumed_agent_notify_launch`, `SessionWiringInputs`, `CliSessionStart` and the project-home main take the path; hooks carry `--daemon-socket` on every host.
- Removed: `socket_for_hooks`, `SessionWiring.hook_socket`; `hooks_report_over_the_agent_tool_socket` takes the spec alone; `record_answer_wait_secs` added for the jailed hooks.
- `bind_owner_only` and the descriptor vetting moved out of `host_session_socket.rs` / `inherited_host_sockets.rs` to `tddy_daemon_kernel::owner_socket`.
- Docs: [module-layout.md](../module-layout.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-agent-tool-socket-per-os-user.md).
