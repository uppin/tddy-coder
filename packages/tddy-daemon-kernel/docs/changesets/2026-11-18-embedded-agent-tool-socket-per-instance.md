# 2026-11-18 — The embedded agent-tool socket is per instance
**Type:** Fix

- `agent_tool_socket.rs`: `next_embedded_socket_path()` (`$TMPDIR/tddy-agent-tools-<pid>-<n>.sock`)
  replaces `agent_tool_socket_path(data_dir)`. `clear_dead_instances_sockets()` removes sockets whose
  pid is gone, through `owner_socket::clear_stale_socket`.
- `agent_tool_sockets.rs`: `Embedded` binds lazily in `acquire_agent_tool_socket`, the same way as
  `PerOsUser` (`0600`, identity-checked, rebinds when its file is gone). The connect-only
  `embedded_socket` probe is gone. `Drop for AgentToolSockets` calls `stop_serving`.
- Tests: two instances on one data dir each serve their own socket; one exiting leaves the other
  served; files are removed on stop; a removed file is rebound; dead instances' files are cleared and
  a live process's are kept.
