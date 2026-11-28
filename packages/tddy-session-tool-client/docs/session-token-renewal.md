# Session-token renewal (tddy-session-tool-client)

A session agent's own token lives `tddy_github::AGENT_TOKEN_TTL` — 24 h — and the agent holds no
refresh token. `tddy-tools --mcp` renews it while it runs, so a session that outlives one minting
keeps its tools ([`src/session_token.rs`](../src/session_token.rs)). Before this existed, an agent
whose token expired lost every tool call at once, and an orchestrator — no shell — had no way out.

## How a renewal runs

`spawn_session_token_renewal`, started by `tddy-tools --mcp` beside its tool server, checks every
`RENEWAL_CHECK_INTERVAL` (15 min). A check asks `renew_session_token_if_due`:

- **Not due** — the token has more than `RENEWAL_MARGIN` (6 h, a quarter of the TTL, room for a
  laptop that sleeps through several checks) left.
- **Due** — the reserved exec-tool call `tddy.token.RenewAgentSession` goes to the agent's **own**
  daemon, the one that signed the token, over the agent-tool UDS or the daemon HTTP route. The
  request names no `daemon_instance_id`, so nothing forwards it: a codebase host cannot renew a
  token another host issued. The daemon answers a fresh `AGENT_TOKEN_TTL` token and its expiry; the
  renewed token is held in this process and **every later tool call carries it**
  (`current_session_token`, in place of the bare `TDDY_REMOTE_SESSION_TOKEN` read). A failed
  renewal is logged and retried at the next check.
- **Not renewable** — this process reaches no daemon that renews: a sandbox socket (authenticated
  by the connection itself) or a room transport. The renewal loop ends.

The first check runs at start with the expiry unknown, so a spawned agent renews once immediately.

The daemon's side of the call is `tddy-daemon-rpc`'s `exec_tool/agent_token_renewal.rs`, and the
re-mint itself is `tddy_daemon_auth::SessionTokens::renew_agent` — see
[tddy-github session tokens](../../tddy-github/docs/session-token.md) for the token format and the
lifetime, and [tddy-daemon-auth auth-service.md](../../tddy-daemon-auth/docs/auth-service.md) for
the renewal's refusals.

## Tests

```bash
cargo test -p tddy-session-tool-client --test session_token_renewal_acceptance
```

`tests/session_token_renewal_acceptance.rs` — a real Unix-socket daemon stub that accepts one token
and retires it on renewal: a token near expiry is renewed under the spawn token, addressed to the
agent's own daemon with no `daemon_instance_id`, and a later call carrying the renewed token
succeeds after the old one stopped working; a token with plenty of life left draws nothing from the
daemon.
