# Cross-daemon session authentication (daemon)

## Purpose

Authenticate a web client against **any** daemon in a LiveKit deployment with a single GitHub login, and keep that session durable across device sleep / tab-background without forcing re-login. Two stateless, HMAC-signed tokens carry the GitHub identity: a short-lived **access token** used on every RPC, and a long-lived **refresh token** used only to mint fresh access tokens. Every daemon verifies both independently using the secret they all share, so no per-daemon session store or cross-daemon session propagation is required.

## Problem this replaces

Previously the `session_token` was an opaque `Uuid::new_v4()` resolved against a **per-daemon** in-memory `HashMap<token, GitHubUser>` (persisted to `<tddy_data_dir>/auth-sessions.json`). The browser logs in once against the *serving* daemon and reuses that one token for every daemon. Peer daemons never saw the token in their local map and rejected it with `invalid or expired session` — breaking daemon switching in the UI, peer project aggregation, and the `StartSession` / `AddProjectToHost` peer-forwarding paths. The LiveKit transport token was never at fault (it is room-scoped and daemon-agnostic).

That stateless single-token design (below, unchanged) still had a client-side gap: the 5-minute access token was refreshed only by a client `setInterval`, which browsers/OSes suspend while a tab is backgrounded or a device sleeps. Once the token lapsed, `RefreshSession` rejected the already-expired token and forced a full re-login — see [Durable sessions](#durable-sessions-access--refresh-tokens) for the fix.

## Token model

- **Format:** `v1.<base64url(payload)>.<base64url(tag)>` where `payload` is JSON `{ id, login, avatar_url, name, iat, exp, kind }` and `tag = HMAC-SHA256(secret, "v1.<base64url(payload)>")`.
- **Kind:** `kind` is `"access"` or `"refresh"` (missing `kind` — a pre-upgrade token already in a browser — deserializes as `"access"`, so existing sessions keep working).
- **Signing key:** `livekit.api_secret` — the one secret identical on every daemon (it already signs LiveKit room JWTs). No dedicated signing-key config is introduced.
- **Verification:** strip the `v1.` prefix, recompute the tag, compare in constant time (`subtle::ct_eq`), then reject if `now > exp`. On success the four GitHub identity fields (plus `kind`) are recovered from the payload — no lookup.
- **Expiry:** an **access** token has a short **5-minute** TTL; a **refresh** token has a **7-day sliding** TTL (each successful `RefreshSession` mints a new refresh token dated 7 days out, so an actively-used session never has to re-login — see [Durable sessions](#durable-sessions-access--refresh-tokens)).

## Behavior

### Minting (login)
`ExchangeCode` completes the GitHub OAuth handshake and returns **both** a freshly signed access token and a freshly signed refresh token. Nothing is stored server-side.

### Verification (every RPC)
`ConnectionService` (and `ActionService` / `TaskService`) gate each call on the same `session_token`. The daemon's session-user resolver **verifies the signature and expiry, and requires `kind == access`** — rather than looking the token up in a local map. Any daemon holding the shared secret accepts a token any other daemon minted; a refresh token presented as an RPC token is rejected.

### Refresh
`RefreshSession(refresh_token)` verifies a currently-valid, **refresh**-kind token and returns a new access token **and** a new refresh token (sliding 7-day window). An access-kind token, an expired token, or a forged token is rejected — the client must re-login. See [Durable sessions](#durable-sessions-access--refresh-tokens) for why this is a two-token exchange rather than the older single-token refresh.

### Logout
Client-side only (clear both stored tokens). Signed tokens are not tracked server-side, so there is no entry to remove; the short access-token TTL bounds the lifetime of a leaked access token, and the 7-day refresh-token TTL bounds a leaked refresh token.

## Durable sessions (access + refresh tokens)

**Problem.** The web client's own periodic refresh (`setInterval`) is suspended by the browser/OS while a tab is backgrounded or the device sleeps. Once the 5-minute access token lapsed, `RefreshSession` had nothing but an already-expired token to work with and rejected it, forcing re-login on every sleep — a poor experience for a tool people leave open on a laptop or phone. The GitHub OAuth App in use issues non-expiring user tokens with no refresh token of its own, so "keep the session alive as long as GitHub is valid" has no GitHub-side signal to track; the durable session is instead represented entirely by our own refresh token.

**Session invariant.** An access (RPC) token is never mintable from nothing — there are exactly two sources: a fresh GitHub login (`ExchangeCode`, which mints the first access token *and* the refresh token together), or a valid `refresh`-kind refresh token (`RefreshSession`). The refresh token **is** the durable user session. Two kind checks make this hold in both directions:
- An **access** token cannot mint another access token — `RefreshSession` requires `kind == refresh`, so a stolen 5-minute token dies at its own expiry and cannot be used to extend a session.
- A **refresh** token cannot authenticate an RPC — the per-RPC session resolver requires `kind == access`, so the long-lived credential is useless even if it leaks into a normal request.

When the refresh token itself expires (7 days idle) or is rejected, there is no path to a new access token: the session has truly ended and the client drops to the login screen.

**Client-side gate (`tddy-web`).** Rather than relying solely on a timer, RPC calls are gated behind a request-time-fresh access token:
- `sessionTokenStore` (`packages/tddy-web/src/rpc/sessionTokenStore.ts`) owns both tokens (`localStorage` keys `tddy_session_token` / `tddy_refresh_token`), decodes the access token's `exp` client-side (no signature check — the server remains the sole verifier), and exposes a single-flight `ensureFreshAccessToken()`: concurrent callers share one in-flight `RefreshSession` rather than each triggering their own.
- `authGateInterceptor` (`packages/tddy-web/src/rpc/authGateInterceptor.ts`) is a ConnectRPC interceptor wired into the HTTP transport (`transportProvider.tsx`) that, before forwarding a request whose message carries a `sessionToken` field, awaits `ensureFreshAccessToken()` and rewrites the field. Because the token is a per-request body field already threaded through every RPC call site, this one interceptor makes all of them self-heal on wake with no call-site changes; `RefreshSession`'s own request carries a `refreshToken` field instead, so it is naturally exempt and cannot recurse through the gate.
- `AuthProvider` owns the shared store, exposes an `isRefreshing` flag, and drives a transparent refresh on mount when the stored access token has expired but the refresh token is still valid — so waking the app does not drop to the login screen. A top-bar indicator (`UserAvatar.tsx`) shows "Refreshing…" while a refresh is in flight.
- Daemon-level RPC over LiveKit is not routed through the ConnectRPC interceptor (LiveKit uses a custom `Transport` without interceptor support); it stays fresh because the shared context `sessionToken` is synced on every refresh. Gating the LiveKit transport itself is a documented follow-up, not required for the fix.

## Security / configuration

- The shared secret is `livekit.api_secret`. When **no** secret is configured the daemon still starts, but auth is non-functional: minting/refresh return an error and the resolver rejects every token (all token-gated RPCs return `Unauthenticated`). **There is no fallback to the legacy local-map behavior.**
- All daemons intended to share sessions must be configured with the **same** `livekit.api_secret` (already required to join the same LiveKit room).
- Verification is constant-time to avoid tag-comparison timing leaks.
- Out of scope: refreshing the GitHub OAuth token itself (the OAuth App's user tokens don't expire and have no refresh token), server-side session revocation, and moving RPC auth from the request body to an `Authorization` header (would touch every daemon service method and all web call sites).

## Related documentation

- [docs/ft/web/daemon-selector-livekit-rpc.md](../web/daemon-selector-livekit-rpc.md) — daemon switching in the web UI (the surface where the original cross-daemon bug appeared).
- [docs/ft/daemon/livekit-peer-discovery.md](livekit-peer-discovery.md) — peer fan-out that forwards `session_token` between daemons.
- `packages/tddy-github/src/session_token.rs` — signer/verifier implementation (`TokenKind`, `mint_access`/`mint_refresh`, `REFRESH_TOKEN_TTL`).
- `packages/tddy-web/src/rpc/sessionTokenStore.ts`, `packages/tddy-web/src/rpc/authGateInterceptor.ts` — client-side durable-session implementation.
