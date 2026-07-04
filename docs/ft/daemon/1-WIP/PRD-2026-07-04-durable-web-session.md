# PRD — Durable web session (refresh-token + RPC token gate)

- **Date:** 2026-07-04
- **PRD type:** Enhancement / bug fix
- **Product areas:** `daemon` (session-auth token model), `web` (auth lifecycle + RPC transport)
- **Status:** 🚧 In Progress — awaiting approval

## Affected features

- [docs/ft/daemon/session-auth.md](../session-auth.md) — cross-daemon stateless session tokens (the token model this PRD extends).
- [docs/ft/web/daemon-selector-livekit-rpc.md](../../web/daemon-selector-livekit-rpc.md) — daemon switching / RPC over LiveKit (consumes the token).
- The shared `AuthProvider` / `useAuthContext` (introduced in #280) — owner of the client-side session lifecycle.

## Summary

Today a web user is silently logged out whenever their device sleeps or the tab is
backgrounded for longer than the **5-minute** access-token TTL — e.g. a phone locked for
5 minutes. This PRD keeps the user's *session* alive across sleep/background without
re-login, by introducing a **long-lived refresh token** (7-day sliding) alongside the
existing short-lived access token, and by **gating RPC calls behind a request-time-fresh
access token** so a call made right after waking transparently refreshes first instead of
failing.

The short-lived access token (5 min) and the stateless, cross-daemon HMAC design are
**unchanged**. The refresh token is a second stateless HMAC token that is never sent on
normal RPCs — it is used only to mint fresh access tokens.

## Background

### Root cause

Two independent failures combine to log the user out (see `packages/tddy-web/src/hooks/useAuth.ts`,
`packages/tddy-github/src/auth_service.rs`):

1. **Timing.** The client refreshes on a `setInterval(4 min)`. Browsers throttle/suspend
   timers in backgrounded tabs, and mobile OSes suspend them entirely while the device is
   asleep — so the pre-expiry refresh never fires in time.
2. **No recovery.** Once the 5-minute access token lapses, the server's `RefreshSession`
   *rejects the expired token* (`Status::unauthenticated`), so there is no path back except a
   full re-login. The client's `catch` handler then wipes the token and drops to the login
   screen.

### Why "as long as the GitHub token is valid" is reframed

The daemon uses a standard **GitHub OAuth App** (`packages/tddy-github/src/real.rs`,
`scope=read:user`). Those user access tokens **do not expire** and GitHub returns **no
`refresh_token`** — the access token is also discarded after login (`_access_token` in
`auth_service.rs`). So "refresh while the GitHub token is valid" has no expiry to track and
nothing to refresh against. The actionable goal is therefore: **keep the web session alive
across sleep/background without forcing re-login, for a bounded idle window, while preserving
the stateless cross-daemon token model.** GitHub-token refresh is explicitly out of scope
(see [Out of scope](#out-of-scope)).

## Proposed changes

### Token model (daemon / `tddy-github`)

Introduce a token **kind** and a two-token pair:

| Token | Kind | TTL | Sent on | Purpose |
|-------|------|-----|---------|---------|
| Access token (today's `session_token`) | `access` | 5 min | every RPC (body `session_token`) | authenticate RPCs |
| Refresh token (new) | `refresh` | 7 days, **sliding** | only `RefreshSession` | mint fresh access tokens |

- `SessionClaims` gains a `kind` field (`"access"` \| `"refresh"`). A token missing `kind`
  decodes as `access` (serde default) so any access token already in a browser keeps working.
- `SessionTokenSigner` grows `mint_access` / `mint_refresh` helpers over the existing `mint`.
- **Login** (`ExchangeCode`) mints **both** tokens and returns both.
- **`RefreshSession`** now accepts a **refresh** token, verifies it (signature + expiry +
  `kind == refresh`), and returns a **new access token (5 min)** and a **new refresh token
  (7 days from now)** — a sliding window: an actively-used session never has to re-login; a
  device untouched for 7 days does.
- **Kind is enforced both ways** (security):
  - `RefreshSession` **rejects an access-kind token** — a stolen 5-minute access token cannot
    be used to extend a session.
  - The daemon's per-RPC session resolver (`packages/tddy-daemon/src/auth.rs`) **rejects a
    refresh-kind token** — the long-lived refresh token cannot authenticate normal RPCs even
    if it leaks into a request.

### Web client (`tddy-web`)

1. **Store both tokens** — add `tddy_refresh_token` alongside `tddy_session_token` in
   `localStorage`.
2. **Single-flight session token store.** A small store owns the access + refresh tokens and
   exposes `ensureFreshAccessToken(): Promise<string>`:
   - returns the current access token when it is present and not within a skew window of its
     `exp` (the client base64url-decodes the token payload to read `exp` — it does not verify
     the signature; the server still does);
   - otherwise performs **exactly one** in-flight `RefreshSession` (concurrent callers await
     the same promise), stores the returned tokens, and resolves with the new access token;
   - on refresh failure (refresh token expired/forged/`Unauthenticated`) → clears both tokens
     and transitions to logged-out.
3. **RPC gate (transport interceptor).** A ConnectRPC interceptor on the HTTP **and** LiveKit
   transports, before each request whose message carries a `sessionToken` field, `await`s
   `ensureFreshAccessToken()` and writes the fresh token into the request. Because the token
   is a per-request body field threaded through ~30 call sites, this one interceptor makes
   **every** call self-heal and "wait" during a refresh — with no change to the call sites.
4. **`AuthProvider`** wires the shared store, drops the sole reliance on the 4-minute interval
   (the gate + `exp`-driven refresh replace it), and exposes an `isRefreshing` flag.
5. **Top-bar refresh indicator.** While a refresh is in flight, the app chrome (near the user
   avatar) shows an unobtrusive "refreshing session" indicator, and hides it when done.

### Proto (`packages/tddy-service/proto/auth.proto`)

- `ExchangeCodeResponse` gains `refresh_token`.
- `RefreshSessionRequest.session_token` → `refresh_token` (same field number 1; the value is
  now a refresh-kind token).
- `RefreshSessionResponse` gains `refresh_token` (new access token stays in `session_token`).

### Session invariant

An access (RPC) token is **never mintable from nothing**. There are exactly two sources: a
fresh GitHub login (`ExchangeCode`, which mints the first access token *and* the refresh
token), or a valid `refresh`-kind refresh token (`RefreshSession`). The refresh token **is**
the durable user session. Corollaries, enforced by kind checks:

- An `access` token cannot mint another access token (`RefreshSession` rejects it) — a stolen
  5-minute token dies at expiry.
- A `refresh` token cannot authenticate an RPC (the daemon resolver rejects it) — the
  long-lived credential is useless as an RPC token.
- When the refresh token expires (7 days idle) or is forged, there is no path to a new access
  token — the session truly ends and re-login is required.

### What stays the same

- Access-token TTL (5 min), wire usage (body `session_token`), and stateless cross-daemon
  HMAC verification.
- The signing secret remains `livekit.api_secret`; no new config.
- No server-side session store is introduced — both tokens remain stateless and self-describing.

## Impact analysis

### Technical

- **Backward compatibility:** an access token already in a browser still authenticates (kind
  defaults to `access`). A browser that has no refresh token (pre-upgrade) simply refreshes
  on its next login — it cannot use the new sliding refresh until it re-logs in once. Acceptable.
- **Security:** the long-lived credential is never sent on normal RPCs and is rejected by the
  RPC resolver; the short-lived access token cannot mint. A leaked refresh token is bounded to
  7 days and to the refresh endpoint only. Constant-time tag comparison is unchanged.
- **Cross-daemon:** refresh tokens are HMAC-signed with the same shared secret, so any daemon
  can mint/verify them — the cross-daemon property is preserved.

### User

- Users stay logged in across sleep/background and daemon switches; re-login is required only
  after **7 days of no use**.
- A brief, self-clearing "refreshing" indicator may appear in the top bar after a long sleep;
  RPCs issued during that window complete once the refresh lands rather than failing.

## Implementation plan

1. `tddy-github`: `kind` claim + `mint_access`/`mint_refresh`; `REFRESH_TOKEN_TTL = 7 days`.
2. `auth.proto` + regenerate: refresh-token fields.
3. `auth_service.rs`: `ExchangeCode` mints both; `RefreshSession` consumes a refresh token,
   enforces `kind == refresh`, returns new access + slid refresh.
4. `tddy-daemon/src/auth.rs`: session resolver rejects refresh-kind tokens.
5. `tddy-web`: session token store (single-flight, `exp`-decode), auth-gate interceptor,
   `AuthProvider` wiring, `isRefreshing`, top-bar indicator, dual-token storage.
6. Tests (acceptance + unit/integration) per the changeset testing plan.

## Acceptance criteria

- [ ] Login returns and the client persists both an access token and a refresh token.
- [ ] `RefreshSession` accepts a valid refresh token and returns a new access token **and** a
      new refresh token whose expiry is ~7 days out (sliding).
- [ ] `RefreshSession` rejects an **access-kind** token.
- [ ] The daemon's per-RPC session resolver rejects a **refresh-kind** token.
- [ ] With an **expired** access token but a **valid** refresh token, the app stays
      authenticated (no login screen) and the next RPC carries a freshly minted access token.
- [ ] Concurrent RPCs needing a refresh trigger **exactly one** `RefreshSession`.
- [ ] An RPC issued while the access token is expired **waits** for the refresh and is sent
      with the fresh token (not the stale one).
- [ ] The top bar shows a "refreshing" indicator while a refresh is in flight and hides it
      afterward.
- [ ] When the **refresh** token is also expired/invalid, the user is logged out (login screen).

## Out of scope

- Refreshing the **GitHub** OAuth token / migrating to GitHub-App expiring user tokens
  (the current OAuth App issues non-expiring tokens with no refresh token).
- Server-side session storage / revocation lists (tokens stay stateless).
- Moving RPC auth from the request body to an `Authorization` header (would touch every
  service method and all ~30 web call sites) — recorded under Future Enhancements.

## References

- [docs/ft/daemon/session-auth.md](../session-auth.md)
- `packages/tddy-github/src/session_token.rs`, `packages/tddy-github/src/auth_service.rs`
- `packages/tddy-web/src/hooks/useAuth.ts`, `packages/tddy-web/src/hooks/authProvider.tsx`
- `packages/tddy-web/src/rpc/transportProvider.tsx`
