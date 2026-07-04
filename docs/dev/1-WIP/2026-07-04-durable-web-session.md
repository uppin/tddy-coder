# Changeset: durable-web-session — refresh-token + RPC token gate

**Date:** 2026-07-04
**Branch:** `feat/session-persistence-github-token`
**Status:** 🚧 In Progress
**Type:** Enhancement / bug fix
**Packages:** `tddy-github`, `tddy-service` (proto), `tddy-daemon`, `tddy-web`
**Feature PRD:** [docs/ft/daemon/1-WIP/PRD-2026-07-04-durable-web-session.md](../../ft/daemon/1-WIP/PRD-2026-07-04-durable-web-session.md)

## Summary

Keep a web user logged in across device sleep / tab-background instead of dropping them after
the 5-minute access-token TTL. Introduce a long-lived **refresh token** (7-day sliding) beside
the short-lived access token, and **gate RPC calls behind a request-time-fresh access token**
so the first call after waking transparently refreshes rather than failing.

## Background

See PRD § Background. Root cause: (1) the client's `setInterval(4 min)` refresh is
suspended while the device sleeps, and (2) the server's `RefreshSession` rejects the
already-expired token, leaving only re-login. The GitHub OAuth App issues non-expiring tokens
with no refresh token, so the durable session is represented by our own HMAC refresh token,
not a GitHub credential.

## Scope

- [ ] Token **kind** (`access` / `refresh`) + `mint_access` / `mint_refresh` + `REFRESH_TOKEN_TTL` (`tddy-github`)
- [ ] `ExchangeCode` mints both tokens; `RefreshSession` consumes a refresh token → new access + slid refresh (`tddy-github`)
- [ ] Proto: `refresh_token` on exchange/refresh messages (`tddy-service`)
- [ ] Daemon per-RPC resolver rejects `refresh`-kind tokens (`tddy-daemon`)
- [ ] Client single-flight session-token store with `exp`-decode (`tddy-web`)
- [ ] Auth-gate transport interceptor rewrites `sessionToken` at send-time (`tddy-web`)
- [ ] `AuthProvider` wiring + `isRefreshing` + dual-token storage (`tddy-web`)
- [ ] Top-bar "refreshing session" indicator (`tddy-web`)

## Technical changes

### State A (current)

- `packages/tddy-github/src/session_token.rs` — `SessionClaims { id, login, avatar_url, name, iat, exp }`;
  single `mint`/`verify`; `SESSION_TOKEN_TTL = 5 min`. No token kind.
- `packages/tddy-github/src/auth_service.rs` — `exchange_code` mints one token and **discards**
  the GitHub access token; `refresh_session(session_token)` verifies a *currently-valid* token
  and re-mints, **rejecting expired tokens**.
- `packages/tddy-service/proto/auth.proto` — `ExchangeCodeResponse { session_token, user }`;
  `RefreshSessionRequest { session_token }`; `RefreshSessionResponse { session_token, user }`.
- `packages/tddy-daemon/src/auth.rs` — session resolver = `signer.verify(token).ok().map(login)`;
  accepts any signature-valid, unexpired token (no kind distinction).
- `packages/tddy-web/src/hooks/useAuth.ts` — one `session_token` in `localStorage`;
  `setInterval(4 min)` refresh; any refresh failure → logout.
- `packages/tddy-web/src/rpc/transportProvider.tsx` — HTTP transport built in
  `createDefaultHttpTransport`; only a traffic-meter interceptor attached.
- Session token is passed as the request-body `sessionToken` field at ~30 web call sites,
  read from `useAuthContext()`.

### State B (target)

- **`session_token.rs`**: `SessionClaims` gains `kind: TokenKind` (`Access` | `Refresh`,
  serde default `Access` for legacy tokens). `mint_access(user)` (5 min) and
  `mint_refresh(user)` (7 days) helpers; `REFRESH_TOKEN_TTL = 7 days`. `verify` returns the
  kind in the claims.
- **`auth_service.rs`**: `exchange_code` mints an access **and** a refresh token, returns both.
  `refresh_session` takes a refresh token, requires `kind == Refresh` (else `Unauthenticated`),
  and returns a new access token + a new refresh token dated 7 days from now (sliding).
- **`auth.proto`**: `ExchangeCodeResponse.refresh_token` (new field); `RefreshSessionRequest`
  field 1 renamed `session_token` → `refresh_token`; `RefreshSessionResponse.refresh_token`
  (new field; new access token stays in `session_token`).
- **`tddy-daemon/src/auth.rs`**: resolver additionally requires `claims.kind == Access` — a
  refresh-kind token is rejected for RPC auth.
- **`tddy-web`**: a `SessionTokenStore` owns `{ accessToken, refreshToken }`
  (`localStorage` keys `tddy_session_token`, `tddy_refresh_token`), decodes the access token's
  `exp`, and exposes single-flight `ensureFreshAccessToken()`. An auth-gate `Interceptor`
  wraps the HTTP + LiveKit transports and rewrites the request's `sessionToken` at send-time.
  `AuthProvider` owns the store, exposes `isRefreshing`, and no longer relies solely on the
  interval. Top-bar indicator subscribes to `isRefreshing`.

### Delta by area

| Area | Change |
|------|--------|
| `tddy-github/src/session_token.rs` | `TokenKind` enum; `kind` claim (serde default `Access`); `mint_access`/`mint_refresh`; `REFRESH_TOKEN_TTL`; `verify` surfaces kind |
| `tddy-github/src/auth_service.rs` | `exchange_code` → access+refresh; `refresh_session` consumes refresh token, enforces kind, slides refresh |
| `tddy-service/proto/auth.proto` | refresh-token fields on exchange/refresh messages (+ regen) |
| `tddy-daemon/src/auth.rs` | resolver rejects refresh-kind tokens |
| `tddy-web/src/rpc/sessionTokenStore.ts` (new) | dual-token store, `exp`-decode, single-flight `ensureFreshAccessToken()` |
| `tddy-web/src/rpc/authGateInterceptor.ts` (new) | interceptor: await fresh token, rewrite `sessionToken` field |
| `tddy-web/src/rpc/transportProvider.tsx` | wire auth-gate interceptor into HTTP + LiveKit transports |
| `tddy-web/src/hooks/authProvider.tsx` / `useAuth.ts` | own the store, dual-token persistence, `isRefreshing`, refresh-token-expiry → logout |
| `tddy-web/src/index.tsx` (top-bar area) | "refreshing session" indicator |

## Testing plan

**Levels.** Token model + kind enforcement is pure Rust logic → **unit tests** in
`tddy-github` and `tddy-daemon`. Client store/interceptor logic is deterministic → **bun:test
unit tests**. End-to-end user-visible behavior (no logout on wake, indicator, gated call
carries fresh token, logout on refresh-token expiry) → **Cypress component acceptance tests**
with `mountWithRpc` + an in-memory two-token backend.

**Why not wire-level / real GitHub.** The OAuth handshake is already covered by the stub
provider; new behavior is entirely in token minting/verification and the client lifecycle, so
in-memory backends give deterministic, fast coverage without a live daemon or GitHub.

**Coverage requirements.** Every PRD acceptance criterion maps to at least one named test below.

### Acceptance tests (Cypress component — `tddy-web`)

New file `packages/tddy-web/cypress/component/DurableSessionAcceptance.cy.tsx`:

- [ ] **"keeps the user authenticated when the access token has expired but the refresh token is still valid"**
      — expired access token + valid refresh in storage; app resolves to authenticated, no
      login screen, and a probe RPC records a freshly-minted access token.
- [ ] **"sends the freshly refreshed access token on an RPC issued while the stored token was expired"**
      — gated probe issues `ConnectionService.ListSessions` with an expired access token; the
      recorded call carries the refreshed token, not the stale one (proves the call waited).
- [ ] **"triggers exactly one RefreshSession when several RPCs fire together with an expired token"**
      — three concurrent gated calls → `backend.callsTo(refreshSession)` has length 1.
- [ ] **"shows a refreshing indicator in the top bar while a refresh is in flight and hides it afterward"**
      — a deferred `RefreshSession` handler held open → indicator visible; resolve → hidden.
- [ ] **"logs the user out when the refresh token is also expired"**
      — `RefreshSession` fails `Unauthenticated`; app drops to the login screen and clears
      stored tokens.

Supporting harness (new): `packages/tddy-web/cypress/support/rpc/durableSessionBackend.ts` —
in-memory `AuthService` implementing the two-token `ExchangeCode`/`RefreshSession`
(refresh returns `access-<n>` / `refresh-<n>`), with a deferrable refresh for the indicator
test; plus a gated-transport mount helper if the gate must be exercised end-to-end.

### Unit / integration tests

Rust — `packages/tddy-github/src/session_token.rs` (`#[cfg(test)]`):

- [ ] **"a minted access token carries the access kind"**
- [ ] **"a minted refresh token carries the refresh kind and a ~7-day lifetime"**
- [ ] **"a legacy token with no kind field verifies as an access token"**

Rust — `packages/tddy-github/src/auth_service.rs` (`#[cfg(test)]`):

- [ ] **"exchange_code returns both an access token and a refresh token"**
- [ ] **"refresh_session mints a new access token and a refresh token that slides ~7 days out"**
- [ ] **"refresh_session rejects an access-kind token"**
- [ ] **"refresh_session rejects an expired refresh token"**

Rust — `packages/tddy-daemon/src/auth.rs` (`#[cfg(test)]`):

- [ ] **"the session resolver rejects a refresh-kind token"**
- [ ] **"the session resolver accepts an access-kind token"**

TypeScript — `packages/tddy-web/src/rpc/sessionTokenStore.test.ts`:

- [ ] **"returns the stored access token unchanged when it is not near expiry"**
- [ ] **"refreshes and returns a new access token when the stored one is expired"**
- [ ] **"performs exactly one refresh when many callers request a token concurrently"**
- [ ] **"persists both the new access and refresh tokens after a refresh"**
- [ ] **"clears both tokens and reports logged-out when the refresh token is rejected"**

TypeScript — `packages/tddy-web/src/rpc/authGateInterceptor.test.ts`:

- [ ] **"rewrites the request sessionToken field with a freshly resolved access token"**
- [ ] **"waits for an in-flight refresh before sending the request"**
- [ ] **"leaves requests without a sessionToken field untouched"**

## Technical debt & production readiness

**Known follow-ups (not blocking):**
- Auth-gate interceptor is wired into the HTTP transport only; LiveKit (daemon) RPCs stay fresh
  via the shared context `sessionToken` (synced on every refresh). LiveKit-transport-level
  gating is a reasonable follow-up but not required by the current behavior.

## Verification (2026-07-04)

- **Rust**: `cargo test -p tddy-github -p tddy-daemon` all pass; `cargo clippy` clean; full
  workspace `cargo build` OK (Rust proto regenerated via `build.rs`).
- **TS units**: `sessionTokenStore.test.ts` + `authGateInterceptor.test.ts` → 8/8.
- **Cypress component suite**: 420/423 across 70 specs. The only failures (3) are the
  pre-existing `PlannedPrRowInternalStatusAcceptance.cy.tsx` — verified failing identically on
  clean `master` (it mounts `SessionsDrawerScreen` without `withSelectedDaemon`, so
  `useDaemonClient` is null); unrelated to this changeset.

## Validation Results

### validate-changes (2026-07-04)

**Critical: 0 · Warning: 1 (fixed) · Info: 2**

- **[WARNING → fixed]** `packages/tddy-web/src/hooks/useAuth.ts` — on-mount `establishSession` returned
  the local (possibly expired) access var; under the production HTTP auth-gate, `getAuthStatus`
  refreshes storage mid-call, so ungated LiveKit/daemon RPCs (which read the context `sessionToken`)
  could send a stale token in the wake-from-sleep case. Masked by tests (`mountWithRpc` omits the
  gate). Fixed to return the authoritative `storage.getAccess()`.
- **[INFO → hardened]** `packages/tddy-github/src/auth_service.rs::get_auth_status` did not check
  token kind; now requires `kind == Access` for consistency with the daemon RPC resolver.
- **[INFO]** `packages/tddy-web/src/components/UserAvatar.tsx` — the `SessionRefreshIndicator`
  element has no direct test; acceptance coverage asserts the `isRefreshing` flag that drives it.

### Test quality / production readiness / clean code

- Tests fluent-compliant (Given/When/Then, page objects, one behavior each), deterministic, real
  assertions. Rewritten `AuthProviderRefreshAcceptance` preserves its original intents under the
  two-token model. No TODO/FIXME markers, no mock/hardcoded production paths, no unsafe fallbacks
  (the "return stored access as-is when no refresh token exists" path is documented and lets the
  server decide — not error-masking), no secrets. New modules are small and single-responsibility.

### Lint / type / test (post-fix)

- `cargo fmt` applied; `cargo clippy -p tddy-github -p tddy-daemon --all-targets -- -D warnings` clean.
- `cargo test -p tddy-github -p tddy-daemon` all pass; TS units 8/8; Cypress auth+app specs 16/16;
  full Cypress component suite 420/423 (3 pre-existing `PlannedPrRow` failures, confirmed on master).
- Whole-workspace `cargo build` succeeds (no compile breaks from the proto change).

## Decisions & trade-offs

- **Two-token model over a grace-window on the existing token** — separates the long-lived
  minting credential from the short-lived RPC credential; a leaked 5-min token is never
  revivable. (User decision.)
- **7-day sliding refresh window** — active users never re-login; 7 days idle → re-login.
  (User decision.)
- **Transport interceptor gate over per-component awaits** — one interceptor makes all ~30
  call sites self-heal and "wait" during refresh with no call-site edits. (User requirement:
  "RPC calls should wait until valid tokens are received.")
- **Keep body-field token; do not move to `Authorization` header** — header-based auth would
  touch every daemon service method and all web call sites; parked as a Future Enhancement.
- **Client decodes the token `exp`** (no signature check client-side) to know when to refresh —
  robust to sleep, avoids a wall-clock guess; the server remains the sole verifier.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 5/5 failing behaviorally, 2026-07-04
- [x] USER REVIEW — acceptance tests (approved 2026-07-04)
- [x] TDD Red — write failing unit/integration tests (Rust + TS, all failing for missing-API reasons)
- [x] TDD Green — implement with quality code (2026-07-04)
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run all tests — targeted suites 100% pass (whole-workspace `./test` re-run in progress after disk cleanup)
- [x] Validate changes (1 warning + 1 info fixed)
- [ ] USER REVIEW — development complete
- [x] Linting and type checking (`cargo fmt`, `clippy -D warnings` clean)
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps
