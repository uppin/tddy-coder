# 2026-11-28 — The agent's tddy-tools renews its own session token

**Type:** Feature · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

`src/session_token.rs`: `spawn_session_token_renewal` — started by `tddy-tools --mcp` beside its
tool server — checks every 15 min and renews the agent's token over its own daemon (the reserved
`tddy.token.RenewAgentSession` exec-tool call, over the agent-tool UDS or the daemon HTTP route,
naming no `daemon_instance_id`) once less than 6 h of the 24 h is left, and once at start with the
expiry unknown. The renewed token is held in this process and every later tool call carries it:
`detect_session_tool_transport` reads `session_token::current_session_token()` in place of the bare
`TDDY_REMOTE_SESSION_TOKEN` environment read. A failed renewal is logged and retried; a process
with no renewing daemon route (a sandbox socket or a room transport) ends the loop. See
[session-token-renewal.md](../session-token-renewal.md).
