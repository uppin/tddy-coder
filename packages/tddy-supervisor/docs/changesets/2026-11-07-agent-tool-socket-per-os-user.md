# 2026-11-07 — agent_tool_sockets: per-OS-user agent-tool sockets handed to a service

**Type:** Feature

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

- `ManagedService.agent_tool_sockets: Vec<HostSocket>` (default empty). `host_socket_config::validate` checks it with `host_sockets` through `validate_kind`: own `socket:` required, allowed non-root plain user, one per user, a directory no other socket shares; both kinds together at most 15 (`MAX_HANDED_OVER_LISTENERS` 16 incl. fd 3).
- `Supervisor::new` binds each with `host_socket::bind_host_socket`; `ServiceSlot.agent_tool_listeners`; `SocketHandover::with_agent_tool_socket` hands them after the host sockets as `agent-tools.<user>` (`AGENT_TOOLS_FD_NAME_PREFIX`, `agent_tools_fd_name`, re-exported).
- Tests: validation parity in `host_socket_config.rs`; `creates_every_declared_agent_tool_socket_when_it_is_built`, `agent_tool_listeners_are_announced_as_agent_tools_user`, `the_agent_tool_fd_name_prefix_is_not_the_host_session_one` in `supervisor.rs`.
- Docs: [host-sockets.md](../host-sockets.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-agent-tool-socket-per-os-user.md).
