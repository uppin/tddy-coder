# 2026-11-07 — AgentToolSockets and the shared owner-only socket code

**Type:** Feature

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

- `agent_tool_sockets`: `AgentToolSockets` (placements `Embedded` / `PerOsUser`; `install_roster`, `adopt_inherited`, `ensure_bound`, `ensure_bound_blocking`, `stop_serving`), `agent_tool_socket_path_for_user` (`<data dir>/run-agent-tools/<user>/agent.sock`, ≤ 103 bytes), `agent_tool_fds`, `adopt_from_environment` / `adopt_descriptors`, `AGENT_TOOLS_FD_NAME_PREFIX`.
- `owner_socket`: `bind_owner_only`, `ensure_dir`, `clear_stale_socket`, `chown`, `undo`, `descriptors_named`, `adopt_descriptor`, `vet`, `require_listening` — moved from `tddy-agent-launch`, with `OwnerSocketKind` naming the refusal's setting.
- `stop_serving` removes only sockets this daemon bound; a supervisor-made one stays (fixed in this PR).
- Removed: `embedded_host_agent_tool_socket` and its 3 unit tests. `tddy-stdio` moved to `[dependencies]`.
- Tests: `src/agent_tool_sockets_tests.rs`.
- Docs: [daemon-kernel.md § Per-OS-user sockets](../daemon-kernel.md#per-os-user-sockets-owner_socket-agent_tool_sockets).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-agent-tool-socket-per-os-user.md).
