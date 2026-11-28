# 2026-11-28 — The agent token's lifetime and its renewal seam

**Type:** Feature · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

`AGENT_TOKEN_TTL` (24 h) names the lifetime of a session agent's own token at the crate root,
beside `SESSION_TOKEN_TTL` and `REFRESH_TOKEN_TTL`; `tddy-session-split`'s
`SPLIT_AGENT_TOKEN_TTL` is now an alias of it. `SessionTokenSigner::mint_agent_with_issued_at` is
the clock seam beside `mint_kind_with_issued_at` (expiry = issued-at + ttl), and
`SessionTokenVerifier::unverified_expiry` reads a refused token's claimed expiry for diagnostics —
to pick the message, never to decide trust. An agent's token outlives one minting through renewal:
the agent's `tddy-tools --mcp` renews it over the daemon route while it is still valid, and only
the daemon that signed it renews it. See [session-token.md](../session-token.md).
