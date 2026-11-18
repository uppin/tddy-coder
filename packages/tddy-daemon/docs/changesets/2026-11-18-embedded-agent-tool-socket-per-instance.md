# 2026-11-18 — The embedded agent-tool socket left the daemon crate
**Type:** Fix

- Removed `src/agent_tool_socket.rs` (`serve_agent_tool_socket`) and `RuntimeTasks::agent_tool_socket`.
  `build_context` only installs the roster, and `AgentToolSockets` binds the embedded socket on first
  use.
- `runtime` re-exports `DaemonChildren`, so the desktop host can stop the same children the binary
  stops.
- `unbundle_endpoint` lists 40 modules. `first_login_enrolment_acceptance` reaches the socket through
  the session host's `AgentToolSockets`.
