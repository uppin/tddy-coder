# 2026-11-28 — SPLIT_AGENT_TOKEN_TTL is the agent token's TTL

**Type:** Refactoring · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

`SPLIT_AGENT_TOKEN_TTL` is now an alias of `tddy_github::AGENT_TOKEN_TTL` rather than its own
86 400 s constant — one name for the lifetime of a session agent's own token, which the agent's
`tddy-tools --mcp` renews before it runs out. The doc comment says so: it is the lifetime of each
token, not of the agent's access, and a resumed session still mints a fresh one.
