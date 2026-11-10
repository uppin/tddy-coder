# 2026-11-09 — The host-session service rides on each user's one daemon socket

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `host_session_socket.rs`: `HostSessionSockets::serve_on(agent_tool_sockets)` registers `HostSessionUserService` as a `UserScopedService`; `ensure_bound` then answers the user's daemon socket path. An embedded host keeps binding `<data>/run/<u>/host.sock`.
- `agent_tool_socket_of` answers the merged path (`user_socket_path_for_user`).
- `inherited_host_sockets.rs` and `HostSessionSockets::adopt_inherited` have no production caller; backlogged as `2026-11-09-dead-inherited-host-session-socket-adoption` (deletion needs consent).
- Docs: [module-layout.md](../module-layout.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
