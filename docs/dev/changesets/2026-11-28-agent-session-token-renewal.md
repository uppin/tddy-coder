# 2026-11-28 — An agent's session token is renewed before its 24 h TTL

**Type:** Feature · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)
**Packages:** `tddy-github`, `tddy-daemon-auth`, `tddy-session-tool-client`, `tddy-session-agents`,
`tddy-session-split`, `tddy-tools-cli`, `tddy-supervisor`, `tddy-daemon-rpc`

Orchestrator `01a20f10-4a30-7df1-8c63-d12aec760d68` was respawned 2026-11-26T21:18:35Z and its
session token — minted only at spawn or on resume, nothing renewing it while the agent ran —
expired 24 h later. Its last successful tool call was at 21:16:26Z, the first refusal at 21:23:29Z,
and the daemon logged 22 refusals for the session through the next morning. The orchestrator has no
shell, so it could not read, start or message any session for ten hours, and the refusal blamed
split-session key advertisement for what was a plain expiry. No other session was refused in that
window.

The fix gives an agent's own token a renewal path instead of a longer TTL:

- **Client.** `tddy-tools --mcp` (`tddy-tools-cli`) spawns
  `tddy_session_tool_client::session_token::spawn_session_token_renewal`: it renews once at start,
  then whenever less than 6 h of the 24 h is left (checking every 15 min). The request is the
  reserved exec-tool call `tddy.token.RenewAgentSession`, sent over the agent-tool UDS or the
  daemon HTTP route, naming no `daemon_instance_id`; every later call carries the renewed token.
- **Daemon.** `tddy-daemon-rpc` answers that name in `ports.rs` before routing — never forwarded,
  refused over LiveKit or when addressed to another daemon — and calls
  `tddy_daemon_auth::SessionTokens::renew_agent`, which accepts only a still-valid agent access
  token signed by **this** daemon for the named session and mints a fresh `AGENT_TOKEN_TTL` token
  with the same identity, principal and session. An expired token is never revived.
- **TTL.** `tddy_github::AGENT_TOKEN_TTL` (24 h) is the one constant;
  `tddy_session_split::SPLIT_AGENT_TOKEN_TTL` aliases it.
- **Refusal message.** `tddy_session_agents::authorize_exec_tool_caller` reads the unverified `exp`
  of a refused token: an expired one is refused as expired, with how long ago and the way out
  (resume the session), instead of the split-session hint.
- **Unrelated build fix.** `tddy-supervisor/src/host_socket.rs` casts `DIRECTORY_MODE` to
  `libc::mode_t`, which `fchmod` takes as `u16` on macOS — master did not compile there.

Detail: [tddy-github session tokens](../../packages/tddy-github/docs/session-token.md),
[tddy-daemon-auth auth-service.md](../../packages/tddy-daemon-auth/docs/auth-service.md),
[tddy-session-tool-client session-token-renewal.md](../../packages/tddy-session-tool-client/docs/session-token-renewal.md),
[tddy-daemon-rpc architecture.md](../../packages/tddy-daemon-rpc/docs/architecture.md).

## Tests

`tddy-daemon-auth` `agent_token_renewal::tests` (5) — renewed past the old expiry, expired not
revived, user token refused, wrong session refused, foreign daemon refused with the exact
unknown-key-id error. `tddy-session-tool-client`
`tests/session_token_renewal_acceptance.rs` (2, real UDS) — renewed near expiry with the spawn
token and the later call working past the old expiry; not renewed with plenty of life left.
`tddy-daemon-rpc` `tests/agent_token_renewal_acceptance.rs` (2) — the agent's token renewed and the
renewed token accepted in turn; the operator's token refused. `tddy-session-agents`
`exec_tool_caller_tests` (2) — an expired token refused as expired with the way out; an
unparseable token keeping the verification hint.

Scoped verification: the 8-package sweep on this change ran 1,766 passed with 44 pre-existing
failures in 13 suites unrelated to it (10 `tddy-tools-cli` `subagent_*`, 3 `tddy-daemon-rpc`
`sandboxed_codebase_*` — `/bin/cat: illegal option` starting a split agent on macOS); clippy
`--no-deps -D warnings` clean on `tddy-daemon-auth`, `tddy-session-tool-client`,
`tddy-session-agents` and `tddy-daemon-rpc` after the pass's cleanups (fluent-tests structure
completed, one two-variant test assertion made exact, `renew_session_token_if_due` split to ~18
lines, the reserved-name intercept moved out of `exec_tool/ports.rs`, which the change had pushed
to 501 production lines — back to 499).

## Still open

Operator notification on a refusal for a live session, and recovery from a token that expired
anyway (a laptop asleep through the 6 h margin) — `docs/dev/todo/2026-11-28-orchestrator-dead-end-on-expired-token.md`,
narrowed by this change, stays in the backlog.
