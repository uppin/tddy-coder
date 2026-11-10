# 2026-11-07 — Split and co-located agents reach their user's agent-tool socket

**Type:** Feature

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

- `SplitSessions.agent_tool_sockets`; `agent_tool_socket_of(os_user)` replaces `agent_tool_socket_for_embedded_host`. `spawn_split_agent`, `resume_split_wiring` and `start_sandboxed_codebase_session` take the session's `os_user`.
- `CodingAgentToolHost` gains `agent_tool_sockets`; `tool_server(session_id, os_user, caller_token)` uses `ensure_bound_blocking`.
- Docs: [module-layout.md](../module-layout.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-agent-tool-socket-per-os-user.md).
