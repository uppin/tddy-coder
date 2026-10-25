# 2026-10-24 — An agent principal on session tokens

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). `SessionClaims.principal: Option<TokenPrincipal>`, `TokenPrincipal::Agent` serialised `"agent"`, optional on the wire and covered by the signature; `SessionTokenSigner::mint_agent(user, ttl)`. A user token is byte-for-byte unchanged and an older token reads as a user's. `session_token_v2.rs` 400 → 444 production lines, under budget. See [session tokens](../session-token.md).
