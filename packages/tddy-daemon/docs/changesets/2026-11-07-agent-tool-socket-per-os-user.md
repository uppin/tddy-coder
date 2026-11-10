# 2026-11-07 — The runtime serves the agent-tool socket per OS user

**Type:** Feature

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

- `BuildContext.agent_tool_sockets`: `PerOsUser` for `RuntimeHost::Binary`, `Embedded` for an embedded host; the finished roster installed in `serve_common_room`; the supervisor's `agent-tools.*` descriptors adopted in `mount_host.rs` before the local socket server clears `LISTEN_*`; handed to `DaemonSessionHost::with_agent_tool_sockets` and `DaemonLauncherDeps`.
- Tests: `tests/server_agent_tool_socket_acceptance.rs` (server binds and serves per user; a session is told its own user's socket).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-agent-tool-socket-per-os-user.md).
