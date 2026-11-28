# Changeset: agent-session-token-renewal

PR: #1064 · Incident: orchestrator `01a20f10-4a30-7df1-8c63-d12aec760d68`, 2026-11-27

## Incident (State A)

An agent's own session token lived 24 h (`SPLIT_AGENT_TOKEN_TTL`). It was minted only at spawn or
when the daemon resumed the session, and nothing renewed it while the agent ran. Orchestrator
`01a20f10` was respawned at 2026-11-26T21:18:35Z, and its token expired at 2026-11-27T21:18:35Z.
Its last successful call was at 21:16:26Z and the first refusal at 21:23:29Z. The daemon log has 22
refusals for that session, the last at 09:52:55 EET on 11-28:

```
exec tool … for session 01a20f10-… refused on daemon LT-R1VXTH2V6H: the session token could not be verified here
UNAUTHENTICATED: daemon LT-R1VXTH2V6H could not verify the session token (invalid or expired there); a split session's tools run on the daemon holding the codebase, …
```

The orchestrator has no shell, so it could not read, start or message any session for ten hours.
The refusal also blamed split-session key advertisement for what was a plain expiry. No other
session was refused in that window. The "other orchestrator recovered on its own" report was not
reproduced: the only self-clearing refusals found were this session's own refusals from udoo
(2026-11-26T19:01Z), and those were a transient key-advertisement gap, not an expiry.

## State B

- **Renewal.** `tddy-tools --mcp` runs `session_token::spawn_session_token_renewal`
  (`packages/tddy-session-tool-client/src/session_token.rs`). It renews once at start, then
  whenever less than 6 h of the 24 h is left (checking every 15 min). The request is the reserved
  exec-tool call `tddy.token.RenewAgentSession`, sent over the agent-tool UDS (or the daemon HTTP
  route), naming no `daemon_instance_id`. Every later call carries the renewed token
  (`current_session_token`).
- **Daemon side.** `ExecToolRpcHandler::execute_tool` answers that name before routing
  (`packages/tddy-daemon-rpc/src/exec_tool/agent_token_renewal.rs`). It refuses calls that arrive
  over LiveKit or that are addressed to another daemon. It then calls
  `SessionTokens::renew_agent` (`packages/tddy-daemon-auth/src/agent_token_renewal.rs`), which
  accepts only a still-valid agent access token signed by **this** daemon for the named session,
  and mints a fresh `AGENT_TOKEN_TTL` token with the same identity, principal and session. An
  expired token is never revived.
- **TTL** moved to `tddy_github::AGENT_TOKEN_TTL`; `SPLIT_AGENT_TOKEN_TTL` aliases it.
- **Refusal message.** `authorize_exec_tool_caller` now reads the unverified `exp` of a refused
  token. An expired one gets "daemon X refused the session token: it expired Nh Mm ago … ask the
  operator to resume it" instead of the split-session hint.
- **Unrelated build fix.** `tddy-supervisor/src/host_socket.rs` casts `DIRECTORY_MODE` to
  `libc::mode_t`. `fchmod` takes a `u16` on macOS, so master (7acb08448) did not compile there.

## Remaining (deferred)

- `docs/dev/todo/2026-11-28-orchestrator-dead-end-on-expired-token.md`: operator notification on a
  refusal for a live session, and recovery when a token expired anyway (a laptop asleep through the
  margin).

## Tests

- `tddy-daemon-auth` `agent_token_renewal::tests` (5): renewed past the old expiry, expired token
  not revived, user token refused, wrong session refused, foreign daemon refused.
- `tddy-session-tool-client` `tests/session_token_renewal_acceptance.rs` (2, real UDS): a token
  near expiry is renewed and a later call succeeds after the old token stopped working; a token
  with plenty of life left is not renewed.
- `tddy-daemon-rpc` `tests/agent_token_renewal_acceptance.rs` (2): the agent's token is renewed and
  the renewed token is accepted in turn; the operator's token cannot be renewed into an agent token.
- `tddy-session-agents` `exec_tool_caller_tests` (2): an expired token is refused as expired, with
  the way out; an unparseable token keeps the verification hint.
