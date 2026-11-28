# An orchestrator with an expired token has no way out

**Status:** open, recommendation pending operator decision · **Found:** 2026-11-28

## Problem

An orchestrator has no shell. Every tool it has is `tddy-tools` MCP, gated by
`TDDY_REMOTE_SESSION_TOKEN`. When that token expires (see
`2026-11-28-agent-session-token-expires-after-24h-with-no-refresh.md`), it cannot read, start or
message any session, and it cannot notify the operator through `tddy-tools notify` either. Session
`01a20f10-…` spent ten hours (2026-11-27 21:23Z to 2026-11-28 07:52Z) firing its cron and failing.
Its only signal was chat text the operator saw when they came back.

## Options

| # | Option | Pros | Cons |
|---|--------|------|------|
| A | **MCP child refreshes its own token**: before `exp` (e.g. at 80 % of the TTL) `tddy-tools --mcp` calls a daemon `RefreshAgentToken` RPC over the agent-tool UDS. The UDS is per-OS-user and the peer is checked, so the socket proves who is calling. The current, still-valid token goes along as proof. | No dead end in normal operation. The agent sees nothing. Uses the same channel as every tool call. | A token that has already expired (laptop asleep through `exp`) still needs B or C. |
| B | A refresh tool that needs no valid token, authenticated by the UDS peer and the session's `hook_token` | Recovers even after expiry | `hook_token` is in `.session.yaml` and lives for the whole session, so it is a weaker credential. Needs a security review. |
| C | **Daemon-side re-mint on resume or wake**: when the daemon delivers a resume or wake, it re-mints the token and pushes it to the running MCP child (signal plus a token file in the session dir, read on each call) | Recovers after expiry with no new agent-held credential | The MCP child must read the token from a file instead of env. Only fires when something wakes the session. |
| D | Daemon notices refusals for a live session and notifies the operator | Cheap. Ends the silent ten hours. | Still a dead end. Only shortens it. |
| E | Longer TTL for orchestrators | One-line change | Pushes the cliff back without removing it, and makes a leaked token worth more |

## Recommendation

**A + D now, and C later only if sleep-through-expiry is seen in practice.**

- A removes the expiry in normal operation. The daemon already mints agent tokens
  (`agent_credentials.rs:37`), and the MCP child already holds a UDS to that daemon
  (`TDDY_REMOTE_DAEMON_SOCKET`). Renewal stays scoped to the same session id and principal, so no new
  credential type is added.
- D is the safety net. On a refusal for a session the daemon knows is live
  (`exec_tool_caller.rs:31`), it sends one notification per session per hour: "session X's token
  expired; resume it".

B is not recommended. It turns a session-long secret into a credential that mints tokens.
