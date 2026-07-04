# Cross-daemon session authentication (daemon)

## Purpose

Authenticate a web client against **any** daemon in a LiveKit deployment with a single GitHub login. The app-level `session_token` is a **stateless, HMAC-signed** credential carrying the GitHub identity; every daemon verifies it independently using the secret they all share, so no per-daemon session store or cross-daemon session propagation is required.

## Problem this replaces

Previously the `session_token` was an opaque `Uuid::new_v4()` resolved against a **per-daemon** in-memory `HashMap<token, GitHubUser>` (persisted to `<tddy_data_dir>/auth-sessions.json`). The browser logs in once against the *serving* daemon and reuses that one token for every daemon. Peer daemons never saw the token in their local map and rejected it with `invalid or expired session` — breaking daemon switching in the UI, peer project aggregation, and the `StartSession` / `AddProjectToHost` peer-forwarding paths. The LiveKit transport token was never at fault (it is room-scoped and daemon-agnostic).

## Token model

- **Format:** `v1.<base64url(payload)>.<base64url(tag)>` where `payload` is JSON `{ id, login, avatar_url, name, iat, exp }` and `tag = HMAC-SHA256(secret, "v1.<base64url(payload)>")`.
- **Signing key:** `livekit.api_secret` — the one secret identical on every daemon (it already signs LiveKit room JWTs). No dedicated signing-key config is introduced.
- **Verification:** strip the `v1.` prefix, recompute the tag, compare in constant time (`subtle::ct_eq`), then reject if `now > exp`. On success the four GitHub identity fields are recovered from the payload — no lookup.
- **Expiry:** short **5-minute** TTL. The web client refreshes ~every 4 minutes (see [Refresh](#refresh)). A token unused for longer than its TTL (e.g. a slept tab) is rejected and the user re-logs in.

## Behavior

### Minting (login)
`ExchangeCode` completes the GitHub OAuth handshake and returns a freshly signed token instead of an opaque UUID. Nothing is stored server-side.

### Verification (every RPC)
`ConnectionService` (and `ActionService` / `TaskService`) gate each call on the same `session_token`. The daemon's session-user resolver now **verifies the signature and expiry** and extracts the login, rather than looking the token up in a local map. Any daemon holding the shared secret accepts a token any other daemon minted.

### Refresh
`RefreshSession(session_token)` verifies a currently-valid (unexpired) token and returns a newly signed token with a fresh `exp`. Used by the web client on a timer to keep an active session alive. An already-expired token is rejected — the client must re-login.

### Logout
Client-side only (clear the stored token). Signed tokens are not tracked server-side, so there is no entry to remove; short TTL bounds the lifetime of a leaked token.

## Security / configuration

- The shared secret is `livekit.api_secret`. When **no** secret is configured the daemon still starts, but auth is non-functional: minting/refresh return an error and the resolver rejects every token (all token-gated RPCs return `Unauthenticated`). **There is no fallback to the legacy local-map behavior.**
- All daemons intended to share sessions must be configured with the **same** `livekit.api_secret` (already required to join the same LiveKit room).
- Verification is constant-time to avoid tag-comparison timing leaks.

## Related documentation

- [docs/ft/web/daemon-selector-livekit-rpc.md](../web/daemon-selector-livekit-rpc.md) — daemon switching in the web UI (the surface where the bug appeared).
- [docs/ft/daemon/livekit-peer-discovery.md](livekit-peer-discovery.md) — peer fan-out that forwards `session_token` between daemons.
- `packages/tddy-github/src/session_token.rs` — signer/verifier implementation.
