# Cross-daemon session authentication (daemon)

## Purpose

Authenticate a web client against **any** daemon in a LiveKit deployment with a single GitHub login, and keep that session durable across device sleep / tab-background without forcing re-login. Two stateless, Ed25519-signed tokens carry the GitHub identity: a short-lived **access token** used on every RPC, and a long-lived **refresh token** used only to mint fresh access tokens. Each daemon signs with a keypair of its own, and a token names the key that signed it; any daemon verifies a token against that key — its own, or the public key the signing daemon advertises on the common room — so no per-daemon session store, cross-daemon session propagation or shared secret is required.

## Problem this replaces

Previously the `session_token` was an opaque `Uuid::new_v4()` resolved against a **per-daemon** in-memory `HashMap<token, GitHubUser>` (persisted to `<tddy_data_dir>/auth-sessions.json`). The browser logs in once against the *serving* daemon and reuses that one token for every daemon. Peer daemons never saw the token in their local map and rejected it with `invalid or expired session` — breaking daemon switching in the UI, peer project aggregation, and the `StartSession` / `AddProjectToHost` peer-forwarding paths. The LiveKit transport token was never at fault (it is room-scoped and daemon-agnostic).

That stateless single-token design (below, unchanged) still had a client-side gap: the 5-minute access token was refreshed only by a client `setInterval`, which browsers/OSes suspend while a tab is backgrounded or a device sleeps. Once the token lapsed, `RefreshSession` rejected the already-expired token and forced a full re-login — see [Durable sessions](#durable-sessions-access--refresh-tokens) for the fix.

## Token model

- **Format:** `v2.<base64url(claims)>.<base64url(signature)>` where `claims` is JSON `{ kid, id, login, avatar_url, name, iat, exp, kind }` and `signature` is the 64-byte Ed25519 signature over `"v2.<base64url(claims)>"`. Byte-level detail: [`packages/tddy-github/docs/session-token.md`](../../../packages/tddy-github/docs/session-token.md).
- **Key id:** `kid` is derived from the signer's public key (the first 16 bytes of the SHA-256 of its SPKI DER, base64url) — never assigned — so an id names exactly one key, forever.
- **Kind:** `kind` is `"access"` or `"refresh"`, and is required: a payload without it is malformed.
- **Signing key:** each daemon's own Ed25519 keypair, `signing_key.pem` (mode `0600`) in `auth_storage` — or in `<tddy_data_dir>/auth` when `auth_storage` is unset. Generated on first boot and reused on every later boot. It never leaves the host; only its public half is published. `livekit.api_secret` plays no part in session tokens.
- **Verification:** read the token's `kid` (untrusted — it only says which key to fetch), resolve it to a public key — this daemon's own, or one a peer advertised — check the claims name that key, verify the signature strictly, then reject if `now > exp`. On success the four GitHub identity fields (plus `kind`) are recovered from the claims — no session lookup.
- **Versions:** only `v2` is accepted. A `v1` token (the retired HMAC format) is refused as an unsupported version, with no migration window — a client holding one signs in again.
- **Expiry:** an **access** token has a short **5-minute** TTL; a **refresh** token has a **7-day sliding** TTL (each successful `RefreshSession` mints a new refresh token dated 7 days out, so an actively-used session never has to re-login — see [Durable sessions](#durable-sessions-access--refresh-tokens)).

## Behavior

### Minting (login)
A sign-in completes by one of two GitHub flows, and both return **both** a freshly signed access token and a freshly signed refresh token, with the signed-in `GitHubUser`, and the state of the operator's credential vault. No session is stored server-side; the GitHub token the login granted is retained in the operator's encrypted credential vault ([below](#github-access-token-retention-added-2026-07-26)). The session is the same whichever flow produced it — one implementation completes both, so they cannot drift into different tokens, lifetimes or retention rules.

| Flow | RPCs | A deployment configures | Used by |
|---|---|---|---|
| **Redirect** | `GetAuthUrl` → GitHub → `/auth/callback` → `ExchangeCode` | `github.client_id` **and** `github.client_secret` | a served deployment behind a real callback URL; `redirect_uri` defaults to `http://{web_host}:{web_port}/auth/callback` |
| **Device** | `StartDeviceLogin` → the operator approves a short code at GitHub → `PollDeviceLogin` until approved | `github.client_id` alone — **no secret is sent** | Tddy Desktop, and any public client |

Neither is a fallback for the other: a deployment declares which it is by what it supplies. A daemon holding only a public `client_id` refuses `GetAuthUrl` and `ExchangeCode` with `failed_precondition`, naming the device flow. `PollDeviceLogin` answers pending, slow-down (with a wider interval to obey), denied, expired, or complete with the same triple `ExchangeCode` returns. `github.stub: true` serves both flows from an in-memory provider.

**The flow is declared, never inferred.** A daemon tells its dashboard which flow it serves in `auth_flow`, at `GET /api/config` and in `GetClientConfig`: `"redirect"` or `"device"`, exactly the provider it registered. **Absent** means this daemon serves no GitHub sign-in, and the dashboard offers neither flow. An **unknown** value is an error the dashboard shows, not a flow it guesses at. A completed sign-in missing its user or either token is refused by the dashboard, which stores nothing.

### Who a login is (the `users:` map)
A login GitHub vouches for is minted a session whether or not it is mapped; each token-gated RPC resolves the caller's OS user through `users:` and refuses an unmapped login `permission_denied: user not mapped to OS user`. There is no default arm. On a **served** deployment `users:` is written by whoever installs it. On **Tddy Desktop**, the first login completed from the application's own window is enrolled once against the OS user the application runs as and persisted, and every different login afterwards is refused exactly as on a server — see [Tddy Desktop § Signing in](../desktop/tddy-desktop-tauri.md#signing-in). A login completed over the LiveKit common room or a local socket never enrols. Adding a second account deliberately is `#keyring` 8/9 ([#515](https://github.com/uppin/tddy-coder/pull/515)).

### Verification (every RPC)
`ConnectionService` (and `ActionService` / `TaskService`) gate each call on the same `session_token`. The daemon's session-user resolver **verifies the signature and expiry, and requires `kind == access`** — rather than looking the token up in a local map. A daemon accepts a token another daemon minted once it has learned that daemon's public key from the common room; a token whose key it has not learned is refused, never tried against another key. A refresh token presented as an RPC token is rejected.

### Refresh
`RefreshSession(refresh_token, vault_unlock_key)` verifies a currently-valid, **refresh**-kind token and returns a new access token **and** a new refresh token (sliding 7-day window). An access-kind token, an expired token, or a forged token is rejected — the client must re-login. See [Durable sessions](#durable-sessions-access--refresh-tokens) for why this is a two-token exchange rather than the older single-token refresh. The refresh also carries the lineage's **vault unlock key**, reopens the operator's credential vault with it after a daemon restart, and hands back a rotated key — see [GitHub access-token retention](#github-access-token-retention-added-2026-07-26).

### Logout
Signed tokens are not tracked server-side, so there is no session entry to remove: the client clears its stored tokens, the short access-token TTL bounds the lifetime of a leaked access token, and the 7-day refresh-token TTL bounds a leaked refresh token. `Logout(session_token, vault_unlock_key)` does reach the daemon for the credential vault: it removes this lineage's unlock slot (the last lineage out closes the vault on the daemon), or — for a lineage that never opened the vault — drops the GitHub token its sign-in left waiting.

## Durable sessions (access + refresh tokens)

**Problem.** The web client's own periodic refresh (`setInterval`) is suspended by the browser/OS while a tab is backgrounded or the device sleeps. Once the 5-minute access token lapsed, `RefreshSession` had nothing but an already-expired token to work with and rejected it, forcing re-login on every sleep — a poor experience for a tool people leave open on a laptop or phone. The GitHub OAuth App in use issues non-expiring user tokens with no refresh token of its own, so "keep the session alive as long as GitHub is valid" has no GitHub-side signal to track; the durable session is instead represented entirely by our own refresh token.

**Session invariant.** An access (RPC) token is never mintable from nothing — there are exactly two sources: a fresh GitHub login (`ExchangeCode` or a completed `PollDeviceLogin`, either of which mints the first access token *and* the refresh token together), or a valid `refresh`-kind refresh token (`RefreshSession`). The refresh token **is** the durable user session. Two kind checks make this hold in both directions:
- An **access** token cannot mint another access token — `RefreshSession` requires `kind == refresh`, so a stolen 5-minute token dies at its own expiry and cannot be used to extend a session.
- A **refresh** token cannot authenticate an RPC — the per-RPC session resolver requires `kind == access`, so the long-lived credential is useless even if it leaks into a normal request.

When the refresh token itself expires (7 days idle) or is rejected, there is no path to a new access token: the session has truly ended and the client drops to the login screen.

**Client-side gate (`tddy-web`).** Rather than relying solely on a timer, RPC calls are gated behind a request-time-fresh access token:
- `sessionTokenStore` (`packages/tddy-web/src/rpc/sessionTokenStore.ts`) owns both tokens (`localStorage` keys `tddy_session_token` / `tddy_refresh_token`) and the lineage's vault unlock key (`tddy_vault_unlock_key`, presented on refresh, replaced with the rotated one, sent on logout, cleared with the tokens), decodes the access token's `exp` client-side (no signature check — the server remains the sole verifier), and exposes a single-flight `ensureFreshAccessToken()`: concurrent callers share one in-flight `RefreshSession` rather than each triggering their own.
- `authGateInterceptor` (`packages/tddy-web/src/rpc/authGateInterceptor.ts`) is a ConnectRPC interceptor wired into the HTTP transport (`transportProvider.tsx`) that, before forwarding a request whose message carries a `sessionToken` field, awaits `ensureFreshAccessToken()` and rewrites the field. Because the token is a per-request body field already threaded through every RPC call site, this one interceptor makes all of them self-heal on wake with no call-site changes; `RefreshSession`'s own request carries a `refreshToken` field instead, so it is naturally exempt and cannot recurse through the gate.
- `AuthProvider` owns the shared store, exposes an `isRefreshing` flag, and drives a transparent refresh on mount when the stored access token has expired but the refresh token is still valid — so waking the app does not drop to the login screen. A top-bar indicator (`UserAvatar.tsx`) shows "Refreshing…" while a refresh is in flight.
- Daemon-level RPC over LiveKit is not routed through the ConnectRPC interceptor (LiveKit uses a custom `Transport` without interceptor support); it stays fresh because the shared context `sessionToken` is synced on every refresh. Gating the LiveKit transport itself is a documented follow-up, not required for the fix.

## GitHub access-token retention *(added 2026-07-26)*

The daemon needs a GitHub credential of its own to read PRs on the operator's behalf (the PR-Stack
Chat Screen's PR status — see [PR-Stack live status § Authenticated PR
status](../coder/pr-stack-live-status.md#authenticated-pr-status-added-2026-07-26)). Previously
`ExchangeCode` **discarded** the access token the operator had just granted and every server-side
GitHub read fell back to `GITHUB_TOKEN`/`GH_TOKEN` from the daemon's own environment — unset under
systemd — so a live PR read as "no PR".

- The OAuth authorize scope widened from **`read:user`** to **`read:user repo`** (`repo` is required to
  read PRs on a private repository).
- A completed sign-in — redirect or device flow — retains the access token in **the operator's own
  credential vault**, `auth_storage/credentials-<hex login>.vault` (mode `0600` in a `0700`
  directory). The vault is encrypted at rest: every record — the secret, its label and its
  metadata — is sealed under a random data key, and that key is wrapped under one the operator's
  **vault passphrase** derives (Argon2id), plus once per browser lineage under an **unlock key** that
  lineage holds. The daemon stores neither, so a disk or a backup without one of them is ciphertext.
  Format, derivation and the registry: [`tddy-credentials` credential-store.md](../../../packages/tddy-credentials/docs/credential-store.md).
- **Signing in always completes, and says where the vault stands.** `ExchangeCode`, a completing
  `PollDeviceLogin`, `RefreshSession` and `GetAuthStatus` carry `vault_state`:

  | `vault_state` | What happened to the login's GitHub token | What the dashboard does |
  |---|---|---|
  | `OPEN` | sealed at once; the lineage is handed an unlock key (`vault_unlock_key`) | nothing |
  | `LOCKED` — a vault file exists and has not been opened since this daemon started | held in daemon memory, never written in plaintext | asks for the passphrase |
  | `UNINITIALIZED` — no vault yet | held in daemon memory | asks the operator to choose a passphrase |
  | `NONE` — a stub login, or no `auth_storage` | not retained | nothing |

- **Unlocking.** `UnlockVault(session_token, passphrase, create)` opens the vault (with `create`,
  makes it — at least 8 and at most 1024 characters), seals the waiting token, gives the lineage an
  unlock slot and returns its key. A wrong passphrase is `failed_precondition` naming the lock and
  changes nothing on disk; the operator stays signed in. Past three wrong answers each attempt waits
  longer (2 s, doubling to 15 min) and is told when to retry.
- **A restart need not ask for the passphrase.** The dashboard keeps its unlock key in
  `localStorage` beside the refresh token and refreshes on page load when it holds one; the refresh
  reopens the vault through that lineage's slot, rotates the slot and returns the new key. The key a
  refresh just retired is answered with the same successor for 30 s, so two tabs sharing one stored
  key both keep a working one. A key that no longer opens its slot still refreshes the session, with
  no key and `LOCKED`; a vault that cannot be read keeps the presented key. At most 16 slots per
  vault; the least recently used is evicted.
- **A forgotten passphrase is a reset, never a silent re-initialisation.** There is no daemon-held
  master key. `ResetVault(session_token, new_passphrase)` renames the old file aside
  (`credentials-<hex>.locked-<unix>.vault`, never deleted — the old passphrase still opens it) and
  seals the waiting token into a fresh vault; every other credential must be linked again. A reset is
  refused while the vault is open, and past five set-aside vaults it is refused rather than deleting
  one.
- **Only a fresh sign-in may choose a vault's passphrase, and "fresh" has a lifetime.** Creating or
  resetting a vault needs the GitHub token a sign-in on this daemon left waiting; an access token
  alone cannot, since it crosses plain http. That waiting token and the permission last
  `github.pending_login_ttl_seconds` (see [Security / configuration](#security--configuration));
  past it the token is dropped from memory, choosing a passphrase asks the operator to sign in to
  GitHub again, and the vault is reported as closed as it was.
- **PR status says what to do while the vault is closed** — *unavailable*, with a reason naming the
  unlock, never "no PR" ([PR-Stack live status](../coder/pr-stack-live-status.md#authenticated-pr-status-added-2026-07-26)).
- **The token never leaves the server.** It is not part of the session token and is never returned
  to the client, so it cannot end up in browser storage on a plain-http origin.
- **A login that cannot retain its token fails; one that cannot yet is reported.** A session minted
  without its token is a half-login: the operator appears signed in while every GitHub-backed read
  reports itself *unavailable*, and re-authenticating — the one remedy — is the one action they have
  no reason to attempt. So a failed vault **write** fails the login — the client-visible status names
  only the login and what failed, and the error naming the file path goes to the daemon log — and a
  **closed** vault is never silent: the response says `LOCKED` or `UNINITIALIZED` and the dashboard
  asks for the passphrase.
- **No passphrase or unlock key is logged, stored in plaintext or returned** beyond the one key a
  lineage is handed. The passphrase requests print redacted.
- **A stub/demo login stores nothing** and is never a failure: `github.stub: true` retains no token by
  construction, reports `NONE`, is handed no unlock key and creates no vault file, and PR lookups for
  it resolve to a clean "no PRs" result. This is enforced by
  `GitHubOAuthProvider::issues_usable_access_token()`, which has no default impl.
- **The trade-off, stated.** An unlock key plus a copy of the disk is the plaintext, and so is the
  passphrase plus a copy of the disk. The unlock key crosses the plain-http LAN origin at every
  login, refresh and logout and sits in `localStorage`; the passphrase crosses it in `UnlockVault` /
  `ResetVault`. Rotation bounds a copied key against the live file only — an old backup keeps its old
  slots, and a set-aside vault keeps the slots and passphrase it had. A lineage that stops refreshing
  without logging out keeps the vault open on the daemon until it exits.

### Operator migration (breaking for running deployments)

**`github-tokens.json` is not read, migrated or renamed** — the plaintext store it belonged to is
deleted (`#keyring` 3/9, [#510](https://github.com/uppin/tddy-coder/pull/510)). The first real login
after upgrading reports `UNINITIALIZED` and asks the operator to choose a vault passphrase; the old
file can be deleted by hand. The steps below are from the change that introduced retention:

1. **Every already-signed-in operator must log out and log in again.** Tokens minted before this change
   carry only `read:user`; there is no way to widen an existing grant in place. Until the re-login,
   PR-backed rows read "PR status unavailable" (with the reason) rather than silently claiming no PR
   exists.
2. **`auth_storage` must exist and be writable by the daemon user.** It is no longer an inert setting:
   - A **configured** `auth_storage` is probed at boot (`build_auth_entries` → `probe_writable`: create
     the directory, write a probe file, remove it). If the probe fails the daemon **refuses to start**,
     with a message naming the path — because retention is now a hard login dependency, an unwritable
     path would otherwise fail *every* login one operator at a time for a fault fully visible at boot.
     There is no fallback to a store-less service.
   - An **unset** `auth_storage` is a deliberate deployment choice, not a failure: the daemon starts,
     logins succeed, and GitHub-backed reads report themselves *unavailable*.
   - `./install` creates and chowns only the parent `/var/lib/tddy` on the root/systemd path, so the
     configured directory (default `/var/lib/tddy/auth`) is a reachable misconfiguration — create it
     and chown it to the daemon user when upgrading.

## Security / configuration

- **`github.pending_login_ttl_seconds`** — how long a sign-in's GitHub token waits in daemon memory for its credential vault, and so how long that sign-in may choose the vault's passphrase. Absent → **600** (ten minutes); `0` → never (held until an unlock, a logout or a restart, and the daemon warns at startup); at most **604800** (seven days, the refresh window). A negative, non-numeric or larger value fails the config load, naming the setting. A sweep drops an expired token at least once a minute. `desktop.yaml.production`, `dev.daemon.yaml` and `dev.desktop.yaml` write it explicitly; `daemon.yaml.production` shows it in its commented `github:` example.
- **Authentication needs no `livekit:` block.** A daemon with a `github:` block signs and verifies session tokens whether or not LiveKit is configured, so a sign-in completes and every token-gated RPC — including the settings service an operator repairs the configuration from — answers. `livekit.api_secret` signs LiveKit room JWTs and nothing else.
- **The key file is guarded, not repaired.** It is written at mode `0600` before any byte of it exists (via `write_atomic_with_mode`), and a key file another account can read is refused at startup rather than tightened — it may already have been read, and the operator is the one to judge that. A present but unparseable key is also refused; the daemon never regenerates over an identity peers have learned. An `auth_storage` directory more permissive than `0700` is warned about once at startup and left as the operator set it.
- **The key location has no guess.** `auth_storage` when configured, else `<tddy_data_dir>/auth`; the daemon refuses to start when neither is known. Moving `auth_storage` moves the key: a daemon that finds no key generates a new identity, and every session it issued ends.
- **Sharing sessions across daemons means sharing a common room.** Peers learn each other's public keys from the `livekit.common_room` advertisement, so daemons that should accept each other's tokens must join the same room. A single daemon, or a desktop install, needs no room: it verifies its own tokens with its own key.
- **Which participants' keys are believed** is decided by one rule both the mint and discovery read — see [LiveKit peer discovery § Trust model](livekit-peer-discovery.md#trust-model). A signed-in web user cannot be minted an identity discovery would read a key from.
- **Revocation costs a restart.** A peer key, once learned, is remembered for the life of the verifying process — safe against forgery (an id names one key), and it keeps peers' tokens verifying through a common-room reconnect. The trade is that a peer that left the room, was removed, or had its key compromised keeps having new tokens accepted until each verifying daemon restarts. There is no revocation list or expiry on learned keys.
- Out of scope: refreshing the GitHub OAuth token itself (the OAuth App's user tokens don't expire and have no refresh token — for the device flow's token this is still to be confirmed against the live API), server-side session revocation, and moving RPC auth from the request body to an `Authorization` header (would touch every daemon service method and all web call sites).

## Related documentation

- [The identity boundary and the LiveKit service](auth-livekit-services.md) — where the signing, verifying and credential-holding code lives, and how a peer's key reaches the verifier without the LiveKit crate reaching auth.
- [Tddy Desktop § Signing in](../desktop/tddy-desktop-tauri.md#signing-in) — device-code sign-in and first-login enrolment on the desktop.
- [`packages/tddy-github/docs/device-flow.md`](../../../packages/tddy-github/docs/device-flow.md) — both GitHub flows, the provider's polling state machine; [`packages/tddy-daemon-auth/docs/auth-service.md`](../../../packages/tddy-daemon-auth/docs/auth-service.md) — which flow `github:` registers, and first-login enrolment.
- [docs/ft/web/daemon-selector-livekit-rpc.md](../web/daemon-selector-livekit-rpc.md) — daemon switching in the web UI (the surface where the original cross-daemon bug appeared).
- [docs/ft/daemon/livekit-peer-discovery.md](livekit-peer-discovery.md) — peer fan-out that forwards `session_token` between daemons.
- `packages/tddy-github/src/session_token_v2.rs` — the token format, signer and verifier (`KeyId`, `TokenKind`, `mint_access`/`mint_refresh`, `REFRESH_TOKEN_TTL`); `packages/tddy-daemon-auth/src/signing_key.rs` — the daemon's keypair and the `KeyDirectory` port.
- `packages/tddy-web/src/rpc/sessionTokenStore.ts`, `packages/tddy-web/src/rpc/authGateInterceptor.ts` — client-side durable-session implementation; `packages/tddy-web/src/components/CredentialVaultPrompt.tsx` — the passphrase prompt.
- [`packages/tddy-credentials/docs/credential-store.md`](../../../packages/tddy-credentials/docs/credential-store.md) — the credential vault: records, sealed format, key derivation, the registry of open vaults, known limitations.
