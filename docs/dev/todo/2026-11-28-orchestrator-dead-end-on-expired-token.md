# An agent whose token expired anyway still has no way out

**Status:** open · **Found:** 2026-11-28 · **Narrowed by:** #1064 (agent token renewal)

#1064 makes `tddy-tools --mcp` renew its agent token before the 24 h TTL runs out
(`packages/tddy-session-tool-client/src/session_token.rs`). Two gaps remain:

1. **A token that expires anyway.** If the machine sleeps through the 6 h renewal margin, or the
   daemon is unreachable for that long, the token expires. Renewal refuses an expired token
   (`packages/tddy-daemon-auth/src/agent_token_renewal.rs`, on purpose), so the orchestrator is back
   in the dead end. The only way out is a daemon-side resume, which re-mints the token
   (`packages/tddy-agent-launch/src/cli_agent_steps/resumed_wiring.rs:148`).
   Option: on resume or wake, the daemon re-mints the token and hands it to the running MCP child
   (a token file in the session dir, read on each call) without restarting the agent.
2. **Nobody is told.** The daemon logs each refusal
   (`packages/tddy-session-agents/src/exec_tool_caller.rs`) but notifies no one. In incident
   `01a20f10` the operator found out ten hours later. Option: on a refusal for a session the daemon
   knows is live, send one operator notification per session per hour ("session X's token expired;
   resume it"). This is not trivial: the exec-tool gate holds no notification port and no
   rate-limit state.

Rejected: a refresh tool authenticated by the session's `hook_token`. It would turn a session-long
secret into a credential that mints tokens.
