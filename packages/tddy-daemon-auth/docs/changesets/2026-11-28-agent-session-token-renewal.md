# 2026-11-28 — SessionTokens::renew_agent

**Type:** Feature · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

`agent_token_renewal.rs`: `SessionTokens::renew_agent(current, session_id)` re-mints a session
agent's still-valid own token for a fresh `tddy_github::AGENT_TOKEN_TTL`, answering
`RenewedAgentToken` (the token and its expiry). The renewal accepts only a still-valid agent access
token signed by **this** daemon for the session it names, and returns one with the same identity,
principal and session; an expired token is never revived. `AgentTokenRenewalRefusal` carries the
four ways a renewal is refused: `NotVerified`, `NotSignedHere`, `NotAnAgentToken` (a user's token
cannot buy an agent token) and `WrongSession`. Reached through `tddy-daemon-rpc`'s reserved
`tddy.token.RenewAgentSession` exec-tool name. See
[auth-service.md](../auth-service.md#one-key-signs-one-thing).
