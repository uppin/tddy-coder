# 2026-11-07 — DaemonSessionHost carries the agent-tool sockets

**Type:** Feature

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

- `DaemonSessionHost.agent_tool_sockets` (default `PerOsUser`), `with_agent_tool_sockets`, `agent_tool_sockets()`; shared with `LaunchSessions` and `SplitSessions`; `DaemonLauncherDeps.agent_tool_sockets` for the managed agent's tool server.
- `shut_down_children` calls `AgentToolSockets::stop_serving` last.
- Tests: `tests/agent_tool_socket_per_user_acceptance.rs` (host Claude and Cursor hooks over the user's socket), `reads_the_same_agent_tool_descriptor_names_the_supervisor_writes`; golden `host_start_settings_local.json` carries `--daemon-socket`.
- Docs: [module-layout.md](../module-layout.md), [coding-agent-launcher.md](../coding-agent-launcher.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-agent-tool-socket-per-os-user.md).
