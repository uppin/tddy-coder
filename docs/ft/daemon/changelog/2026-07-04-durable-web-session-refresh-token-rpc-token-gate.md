# 2026-07-04 — Durable web session (refresh token + RPC token gate)

- Session tokens split into a short-lived **access** token (5 min, unchanged, sent on every RPC) and a new long-lived **refresh** token (7-day sliding); `ExchangeCode` now mints both, and `RefreshSession` consumes a refresh token to mint a new access token plus a slid refresh token — fixing users being logged out whenever a device slept or a tab was backgrounded past the 5-minute access-token TTL
- Kind is enforced both ways: the daemon's per-RPC resolver rejects a refresh-kind token, and `RefreshSession` rejects an access-kind token, so neither credential can do the other's job; a token with no `kind` (pre-upgrade) still verifies as access-kind
- Web client gates RPC calls behind a request-time-fresh access token (`sessionTokenStore` + `authGateInterceptor`, single-flight refresh) instead of relying solely on a background timer, so the first call after waking transparently refreshes rather than failing; a top-bar indicator shows while a refresh is in flight
- Feature: [session-auth.md § Durable sessions](../session-auth.md#durable-sessions-access--refresh-tokens)
