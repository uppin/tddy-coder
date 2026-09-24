# The identity boundary (tddy-daemon-auth)

Who a session token belongs to, and every credential the daemon holds on that person's behalf —
served as four gRPC services and one function, plus the admission a desktop enrols its first login
through and the credential vaults a login's GitHub token is sealed into.

`AuthBuildResult::user_resolver` is the daemon's single identity function. Every other service, in
every other crate, authenticates with a clone of it. That is what makes this crate the identity
boundary in fact and not only in name.

## Where the code lives

| Module | What is in it |
|---|---|
| `auth` | `build_auth_entries_with` (a daemon with a signing identity), `build_auth_entries_admitting` (the same, with a `LoginAdmission`) and `build_auth_entries` (one without); `GitHubAuthFlow` and `github_auth_flow`; the `auth.AuthService` and `auth.LiveKitTokenService` handlers, `session_token_authenticator`, `build_token_service_entry` |
| `first_login_admission` | `FirstLoginEnrolment` — a desktop's first-login enrolment, the `LoginAdmission` an embedded host is built with |
| `signing_key` | `DaemonSigningKey`, `load_signing_key` / `signing_key_path`, the `KeyDirectory` port and `StandaloneKeyDirectory`, `DirectorySessionTokenVerifier`, `SessionTokens`, `auth_storage_looser_than_owner_only` |
| `vault_lifetimes` | `credential_vaults_in` — the credential vaults over `auth_storage`, holding a pending sign-in for `github.pending_login_ttl_seconds` and an unused open vault for `github.open_vault_idle_ttl_seconds`; `spawn_credential_sweep` and `sweep_period` |
| `github_pr_credentials` | `PrLookup` / `pr_lookup_for_caller`, the three outcomes a PR list reads by, and `retained_github_token`, the caller's GitHub token read from their open credential vault |
| `codex_oauth_relay` | authorize-URL validation and callback parsing — [codex-oauth-relay.md](./codex-oauth-relay.md) |
| `oauth_loopback_tunnel` | the operator-side TCP listener and its LiveKit bridge — [oauth-loopback-tunnel.md](./oauth-loopback-tunnel.md) |
| `codex_oauth_participant_metadata` | the `codex_oauth` participant-metadata shape both halves read |
| `token_provider` | the token-source seam a caller injects |

## Services

| Service | Methods | Notes |
|---|---|---|
| `auth.AuthService` | 9 | GitHub sign-in by the redirect flow (`GetAuthUrl`, `ExchangeCode`) or the device flow (`StartDeviceLogin`, `PollDeviceLogin`), `GetAuthStatus`, `RefreshSession`, `Logout`, the credential vault's `UnlockVault` and `ResetVault`, and what a sign-in retains |
| `auth.LiveKitTokenService` | 1 | `MintLiveKitToken` — a room JWT |
| `token.TokenService` | 2 | session tokens |
| `loopback_tunnel.LoopbackTunnelService` | 1 | `StreamBytes`, the session-host end of the OAuth tunnel |

**No wire coordinate changed when this crate was cut out of `tddy-daemon`.** All four were already
their own protos, so no client migrated — which is why the identity boundary could be drawn early
without a breaking change riding along.

## Which sign-in `github:` registers

`build_auth_entries_admitting` reads the `github:` block through one private decision,
`github_provider_kind`, and registers at most one `auth.AuthService`:

| `github:` holds | Provider | Flow it serves |
|---|---|---|
| `stub: true` | `StubGitHubProvider::new_with_callback`, with `stub_codes` registered through `register_code_mappings` | both; declared as `redirect` |
| `client_id` **and** `client_secret` | `RealGitHubProvider::new(client_id, client_secret, redirect_uri)` — a confidential client | redirect (the device flow works too, and never sends the secret) |
| `client_id` and **no** `client_secret` | `RealGitHubProvider::new_public(client_id)` — a public client | device |
| neither a stub nor a `client_id`, or no `github:` block | none — `AuthBuildResult::unauthenticated()` | none |

These are two configurations, not a fallback: a deployment declares which one it is by what it
supplies. A secret inside a downloadable application is public the day it ships, so a desktop
install is the public-client row. `redirect_uri` defaults to `http://{web_host}:{web_port}/auth/callback`
and feeds only the stub and the confidential client; a public client holds no callback.

**A public client refuses the redirect flow.** Its `GetAuthUrl` and `ExchangeCode` both answer
`failed_precondition`, naming the device flow, so an operator is never sent to GitHub and back for an
exchange that cannot complete.

**The flow is declared, never inferred.** `GitHubAuthFlow { Redirect, Device }` (`as_str()`:
`"redirect"` / `"device"`) is what `github_auth_flow(config)` returns, from the same
`github_provider_kind` the builder reads, so the flow a dashboard is told can never differ from the
provider serving it. `None` is a daemon that registers no `auth.AuthService`. `tddy-daemon` carries
it as `auth_flow` on both client-config paths — `GET /api/config` (`server.rs`, from `main.rs`) and
`GetClientConfigResponse.auth_flow` (`daemon_config_service.rs`) — and omits the field for `None`.

**Device-flow handlers.** `StartDeviceLogin` returns GitHub's device code, user code, verification
URI, expiry and interval, and remembers nothing. `PollDeviceLogin` answers `PENDING`, `SLOW_DOWN`
(with the interval to adopt), `DENIED`, `EXPIRED`, or `COMPLETE` with the same `session_token`,
`user` and `refresh_token` `ExchangeCode` returns. Both flows finish through
`AuthServiceImpl::complete_login`, so a device login is admitted, retained and minted exactly as a
redirect login is — see [`tddy-github` device-flow.md](../../tddy-github/docs/device-flow.md).

## Credential vaults

A real login's GitHub access token is sealed into **that operator's credential vault**,
`auth_storage/credentials-<hex login>.vault` — encrypted under a key the operator's vault
passphrase derives, never in plaintext. The format, the key derivation, the vault states and the
unlock slots are `tddy-credentials`'
([credential-store.md](../../tddy-credentials/docs/credential-store.md)); the RPCs that act on them
are `tddy-github`'s `AuthServiceImpl`. This crate builds the registry and reads from it.

**Construction.** When `auth_storage` is set, `build_auth_entries_admitting` probes it (create the
directory `0700`, write and remove `credentials.probe`), warns once about a directory looser than
`0700`, and builds one `SessionVaults` through `vault_lifetimes::credential_vaults_in(dir, &lifetimes)`. The
same `Arc<SessionVaults>` goes to the `auth.AuthService` entry (`with_credential_vaults`) and back in
`AuthBuildResult::credential_vaults`, which `tddy-daemon`'s runtime hands to the session host for
PR-status reads. No `auth_storage` → `None`: logins succeed, report the vault `NONE`, and PR status
is *unavailable*. A stub provider retains nothing whether or not the vaults exist.

**`github.pending_login_ttl_seconds`.** A sign-in over a closed vault (`LOCKED`, `UNINITIALIZED`)
holds its token in memory, unsealed, until the passphrase opens the vault, and that waiting token is
also what permits choosing a first passphrase or a reset. Both expire:

| `github:` key | Value | Meaning |
|---|---|---|
| `pending_login_ttl_seconds` | absent | 600 — ten minutes |
| | `0` | never: held until an unlock, a logout or a restart; a startup `warn` says so |
| | `1` … `604800` | that many seconds (at most the seven-day refresh window) |

**Where the meaning is decided.** The daemon's config (`tddy-daemon-kernel`) reads both lifetimes
as a plain `Option<u64>` — a negative or non-numeric value fails the config load, naming the
setting — and nothing more. `vault_lifetimes::VaultLifetimes::of(github)`, the first thing
`build_auth_entries_admitting` does once it has a `github:` block, applies the defaults, `0` =
never, and the ceiling, `tddy_github::REFRESH_TOKEN_TTL`: a value past it is an `Err` naming the
setting and its limit (`github.pending_login_ttl_seconds is at most 604800 …`), so **the daemon does
not start** — with or without `auth_storage`. It lives here because this crate already depends on
`tddy-github`, and the kernel must not. `tests/vault_lifetime_config_acceptance.rs` pins it from
yaml through `build_auth_entries`. At startup
`credential_vaults_in` logs the lifetime at `info` (target `tddy_daemon::auth`).
`SessionVaults` checks the lifetime on every access.

**`github.open_vault_idle_ttl_seconds`.** An unlocked vault keeps its data key in memory for the
PR-status reads, and session tokens are stateless, so a lineage that stops coming back without
logging out would keep it open until the daemon exits. So a vault nothing **uses** — a sign-in
sealing into it, an unlock, a refresh reopening or rotating through it, or a credential read
through `retained_github_token` — is closed, and its data key dropped, after:

| `github:` key | Value | Meaning |
|---|---|---|
| `open_vault_idle_ttl_seconds` | absent | 604800 — seven days, `tddy_github::REFRESH_TOKEN_TTL` |
| | `0` | never: held until its last lineage logs out or a restart; a startup `warn` says so |
| | `1` … `604800` | that many seconds (at most the refresh-token lifetime) |

Resolved by `VaultLifetimes::of` like the pending lifetime; `credential_vaults_in` logs it at
`info` at startup. A closed vault is exactly
a restarted daemon's: `LOCKED`, PR status *unavailable* with the reopen reason below, and the next
refresh presenting an unlock key reopens it. Each closing is logged at `info` by `tddy-credentials`
(target `tddy_credentials::sessions`) with the login and how long the vault sat unused.

**The sweep.** `spawn_credential_sweep` — spawned by `runtime::build` where the vaults are injected —
looks every min(pending lifetime, idle lifetime, 60 s): `expire_pending` for a pending token nobody
touches, `evict_idle` for an open vault nobody looks up, each skipped when its lifetime is `0`. It
holds the vaults weakly, ends when the daemon drops them, and is not started at all when both
lifetimes are `0`. (Renamed from `pending_logins::spawn_pending_login_sweep` when it took on the
second kind.)

**The PR-status read.** `retained_github_token(vaults, login)` reads the caller's token by the
vault's state, and names the remedy when it cannot:

| State | Answer |
|---|---|
| no vaults (no `auth_storage`), or `UNINITIALIZED` with no token waiting | `Ok(None)` — `pr_lookup_for_caller` then says to sign in to GitHub again |
| `LOCKED` | `Err` — "your credential vault is locked on this daemon — unlock your credential vault with its passphrase, or it reopens at your next session refresh" |
| `UNINITIALIZED` with a token waiting | `Err` — "your GitHub credential is waiting for a credential vault — unlock your credential vault by choosing its passphrase" |
| `OPEN` | the sealed `github` record for the login, or `Ok(None)`; an unreadable vault is logged with its detail (`tddy_daemon::github_pr_credentials`) and answered with a reason that carries none of it. The read goes through `SessionVaults::use_open`, so it counts as a use and keeps the vault open another idle lifetime |

`pr_lookup_for_caller(stub_mode, stored)`'s three outcomes — `Empty`, `Unavailable(reason)`,
`Perform(token)` — are what the PR list reads by, unchanged by where the token comes from.

## First-login enrolment

A server's `users:` is written by whoever installs it. A login GitHub vouches for is minted a
session whether or not it is mapped, and each token-gated RPC refuses an unmapped caller
`permission_denied: user not mapped to OS user`. A desktop install has nobody to write `users:`, so
its **first** login from its own window is written down as it completes.

`FirstLoginEnrolment::new(users, config_path, os_user)` implements `tddy_github::LoginAdmission`,
which `AuthServiceImpl` asks before it retains or mints anything:

```rust
fn admit(&self, github_login: &str, transport: RequestTransport) -> Result<(), Status>;
```

`transport` is how the completing call — `ExchangeCode` or a completing `PollDeviceLogin` — reached
the daemon, as the host that received it stamped it
([`tddy-rpc` request-transport.md](../../tddy-rpc/docs/request-transport.md)). `admit` decides, in
order:

| Situation | Outcome |
|---|---|
| `github_login` is already mapped | admitted |
| unmapped, and `transport` is anything but `InProcess` | admitted **unmapped**, nothing written; logged. Its RPCs are refused `permission_denied` |
| unmapped, `InProcess`, and `users:` is empty | **enrolled**: the row `github_login → os_user` is persisted to `config_path`, then applied to the shared `LiveUsers`, then the session is minted |
| unmapped, `InProcess`, and `users:` already names somebody (`EnrolmentRefusal::AlreadyEnrolled`) | admitted **unmapped**, nothing written; logged. A second account is not added by signing in |
| the config file cannot be rewritten (`EnrolmentRefusal::ConfigNotWritable`) | refused `failed_precondition`, naming the reason. Nobody is mapped, so the login does not appear to work and then vanish on restart |

**Only `InProcess` enrols.** `is_this_desktops_window` is an **exhaustive** match with no wildcard
arm: `InProcess => true`, and `LiveKit`, `UnixSocket`, `Pipe`, `Http`, `Grpc` and `Direct` all
`=> false`. A transport added later must be decided there rather than inherit enrolment. The same
`auth.AuthService` is served on the LiveKit common room and the agent tool socket too, and a login
completed over either is somebody who is not necessarily at this machine — a room peer, a co-located
process. On an unenrolled desktop such a login writes nothing, and the desktop can still enrol its
own window's login afterwards.

**Which deployments enrol** is decided where the daemon is assembled, not here: `tddy-daemon`'s
`runtime::build` builds a `FirstLoginEnrolment` only for an embedded host started from a config file,
mapped to the OS user the process runs as — see
[daemon-endpoint.md](../../tddy-daemon/docs/daemon-endpoint.md#first-login-admission). A server
never has an admission, and `build_auth_entries_with` is `build_auth_entries_admitting(.., None)`.

The persistence — the file write, the in-memory row every service sees, and the lock that serialises
it against `UpdateConfig` — is `tddy-daemon-kernel`'s
([daemon-kernel.md](../../tddy-daemon-kernel/docs/daemon-kernel.md#users-the-live-holder-and-first-login-enrolment)).
`os_user_for_github` is unchanged by all of it: an unmapped login is still `None`, with no default
arm.

**Known limits.** Enrolment is decided by the transport of the *completing* poll only; the
`StartDeviceLogin` that issued the code is not bound to it. That is not exploitable — a device code is
returned only to its starter — but binding an attempt to its starting transport would be defence in
depth. `admit` does its one file write synchronously under a `std::sync::Mutex` on an async RPC task;
it happens once per deployment, and `spawn_blocking` would be the tidier shape.

## One key signs one thing

Each daemon signs session tokens with an **Ed25519 key of its own**, and
`config.livekit.api_secret` signs LiveKit room JWTs and nothing else. The token format is
`tddy-github`'s ([session-token.md](../../tddy-github/docs/session-token.md)); the key material, its
at-rest posture and the way a peer's key is found are this crate's.

**`DaemonSigningKey`** is the keypair. `load_signing_key(config)` finds it at
`signing_key_path(config)` — `auth_storage/signing_key.pem` when `auth_storage` is set, else
`<tddy_data_dir>/auth/signing_key.pem` — and **refuses** a config naming neither, rather than
repeating the runtime's data-directory rule (`runtime::build` pins its resolved `tddy_data_dir`
before asking). `DaemonSigningKey::load_or_generate`:

- **loads** an existing key and never regenerates over it — a present but unusable file is a
  damaged or exposed identity, and replacing it would destroy the only copy every peer learned;
- **refuses** a key file any other account can read or write, and does not repair it — a key that
  has been readable may already have been read;
- **generates** on first boot through `write_atomic_with_mode` at `0600` into a private staging
  file, then **hard-links** it into place. A link, unlike a rename, refuses to replace an existing
  file, so two daemons booting on one data directory cannot each keep a different key: the loser
  discards its own and loads the winner's.

There is no in-memory fallback: a daemon that cannot persist its identity would mint a new one on
every restart and end every live session.

The key id is derived from the public key (`KeyId::of`), so `DaemonSigningKey::signer()` needs no
second argument and cannot be handed a mismatched id.

**`KeyDirectory`** is the port through which a daemon resolves a *peer's* key:
`async fn public_key_for(&KeyId) -> Result<Option<VerifyingKey>>`. It resolves and does nothing
else — how a daemon's own key reaches its peers is the transport's business, and on a LiveKit fleet
the key rides the common-room advertisement the discovery loop publishes, in a crate this one may
not depend on. `StandaloneKeyDirectory` answers `None` to every id: Tddy Desktop's shape, and every
daemon's without a common room. The fleet implementation, `CommonRoomKeyDirectory`, is
`tddy-daemon`'s ([daemon-endpoint.md](../../tddy-daemon/docs/daemon-endpoint.md)).

**`DirectorySessionTokenVerifier`** reads a token's `kid`, compares it with the local key (held
directly, so a daemon always verifies its own tokens), otherwise resolves it through the directory,
then checks the signature. An unknown id is `UnknownKeyId` — never retried against another key.
It implements `tddy_github::SessionTokenAuthority`, so `AuthServiceImpl`'s status and refresh paths
accept exactly the peers the RPC gate accepts.

**`verify_now` polls once.** Every token-gated RPC authenticates through the synchronous
`SessionUserResolver`, so `verify_now` polls the async verification **exactly once** with a no-op
waker; a lookup still pending is refused as `UnknownKeyId` and dropped, never waited for. That is
safe with the directories that exist — the local key needs no lookup, `StandaloneKeyDirectory`
answers immediately, and `CommonRoomKeyDirectory` reads an in-memory snapshot behind a `std` lock.
A future I/O-backed directory must keep a local view that a background task refreshes and answer
from it; one that awaited inside `public_key_for` would refuse every peer token as unknown, with a
warning naming the key id rather than a loud failure.

**`SessionTokens`** is one signer plus one verifier, built **once per daemon** in `runtime::build`
and handed to `build_auth_entries_admitting`, the local socket, `local_token.LocalTokenService` and the
session host's split and jailed-codebase agent credentials. No path may construct a second key —
that would be an identity no peer was told about. `build_auth_entries(config, …)` remains for a
daemon with no signing identity (no `github:` block), and answers it with `user_resolver: None`, so
the wiring layer registers no session services — a refusal, not a permissive default.

**The LiveKit crate never reaches this one.** Room JWTs cross the boundary through the
`SessionTokenMinter` port, and peers' keys cross it as opaque strings `tddy-daemon` decodes;
`tests/dependency_boundary_unit.rs` on the LiveKit side pins `tddy-daemon-auth` off its dependency
path, so neither port can quietly stop being one.

## This crate is smaller than "auth" suggests

The host-key path — `host_keypair`, `host_private_key`, `ssh_agent`, `ssh_agent_add`, 1,994
production lines — lives in [`tddy-host-service`](../../tddy-host-service/docs/host-service.md),
because `AddHostKey` and `ListHostKeyCandidates` are host-service methods and because that move is
what cut the `host_tooling ⇄ ssh_agent` cycle.

The boundary is real rather than convenient: **what is here signs and verifies; what is there
unlocks and loads.**

`daemon_settings` and `daemon_config_service` are not here either. They serve the daemon's own
configuration, which is wiring.

## Secrets at rest

The signing key and the `auth_storage` probe are written through
`tddy_core::atomic_file::write_atomic_with_mode`; the credential vaults through `tddy-credentials`'
own owner-only swap-then-rename writer (`atomic.rs`, the same shape, inlined so that crate does not
depend on `tddy-core`). Both stage to a swap file and rename. A crash mid-write leaves the previous value intact; an empty secrets file reads as
"no credential", which surfaces to an operator as a re-auth prompt rather than as the write failure
it is — so the truncate-in-place path this crate would otherwise have carried is not one it can
tolerate at its centre.

**The mode-aware variant, not the plain one.** `write_atomic` carries permission bits over from an
*existing* target, so a **first** write through it would create the swap file at the process umask
and publish a world-readable credential store.

**`ensure_owner_only_dir` builds the directory with `DirBuilder::recursive(true).mode(0o700)`**, and
that has two consequences worth stating rather than discovering:

1. An **existing** storage directory does not have `0o700` re-imposed on every write, so the
   daemon never overrules an operator's deliberate `chmod`. An `auth_storage` more permissive than
   `0700` is instead **warned about once, at startup**, by `build_auth_entries_admitting`
   (`auth_storage_looser_than_owner_only`), and left as it is. A directory's mode governs listing
   and traversal, not the contents of the `0600` files inside it, so the signing key and the credential
   vaults are protected by their own modes even in a loose directory.
2. The mode applies to **every** directory the call creates, not just the leaf. With
   `auth_storage = /var/lib/tddy/auth` and no `/var/lib/tddy`, that parent is created `0700` and
   owned by the daemon user rather than taking the process umask. The signing key's directory is
   created the same way.

An unwritable `auth_storage` fails `build_auth_entries` outright. Retention is a hard login
dependency — a failed `put` fails the exchange — so an unwritable path breaks *every* login rather
than merely degrading PR status, and `install` only creates and chowns `/var/lib/tddy` on the
root/systemd path, which makes this a reachable misconfiguration rather than a theoretical one.

## Log targets still name `tddy_daemon`

`tddy_daemon::auth` (`AUTH_LOG_TARGET`, which `tddy-daemon`'s runtime logs its signing identity
under too), `tddy_daemon::codex_oauth`, `tddy_daemon::github_pr_credentials` and
`tddy_daemon::oauth_tunnel` are the targets this crate logs under, and they are kept deliberately: a
log target is an operator's `RUST_LOG` filter, and renaming it to match the crate would silently
break every filter already selecting it. A fleet-wide rename is its own change with its own release
note. `tddy-host-service` set the same precedent in node 1.

## Tests

```bash
cargo test -p tddy-daemon-auth
```

| Suite | Covers |
|---|---|
| `tests/auth_service_acceptance.rs` | `GetAuthUrl`, `ExchangeCode`, `GetAuthStatus`, `RefreshSession` and `Logout` answering from this crate, plus a refusal of a token signed by a key this daemon does not know |
| `tests/device_login_acceptance.rs` | a daemon holding only a public `client_id` registering `auth.AuthService`, refusing `GetAuthUrl` `failed_precondition` and declaring the `device` flow; a stub daemon declaring `redirect`; a started device login handing out the stub's code, verification URI, expiry and interval; an approved login yielding the session `ExchangeCode` would have, whose token then authenticates `GetAuthStatus` |
| `first_login_admission.rs` (inline) | every non-`InProcess` transport admitting an unmapped first login and leaving the file byte-identical; `InProcess` enrolling, in memory and on reload; a config file that has gone refused `failed_precondition` with nobody mapped |
| `tests/token_service_acceptance.rs` | `MintLiveKitToken` and `token.TokenService` minting room JWTs against `config.livekit.api_secret` (its only job), each verified with `livekit_api::access_token::TokenVerifier` — and a server holding a *different* secret refusing the same JWT, without which the positive cases assert nothing |
| `tests/cross_crate_session_token_acceptance.rs` | a token signed here authenticating a call to a service in another crate. The far side is `tddy-service` deliberately: this crate cannot reach the daemon's own services, which is exactly the property below |
| `tests/per_daemon_signing_identity_acceptance.rs` | daemon A's token verifying on daemon B once B's directory holds A's key — B's directory is asked for exactly A's key id — and an unseen key id refused with no fallback |
| `tests/auth_without_livekit_acceptance.rs` | a daemon with no `livekit:` block completing a sign-in through the served `ExchangeCode`, and the token it returns resolving to that login |
| `tests/auth_storage_posture_warning_acceptance.rs` | exactly one startup warning for a `0755` `auth_storage`, none for `0700` |
| `signing_key.rs` (inline) | generate once at `0600` and reuse byte-identically across a restart; a group-readable key refused and left unrepaired; the key's location from `auth_storage` or `tddy_data_dir`, and a refusal with neither; a directory lookup that cannot answer on the first poll refused without waiting. The first-boot hard-link race has no dedicated test |
| `tests/dependency_boundary_unit.rs` | `tddy-daemon` is absent from this crate's transitive manifest closure — with a third test asserting the walk actually reaches `tddy-daemon-kernel`, so a walk that silently found nothing cannot pass as a clean result |
| `tests/login_opens_the_credential_store_acceptance.rs` | a login reporting `OPEN`, `LOCKED` or `UNINITIALIZED` and retaining its token accordingly; after a restart, a fresh login with a **new** token (the fake GitHub in `tests/support/mod.rs` mints one per exchange) `LOCKED`, then opening the same vault with the passphrase; a wrong passphrase `failed_precondition` with the file unchanged; a reset setting the old vault aside; a stub login creating nothing and reporting `NONE`; no passphrase in any log line, file or response |
| `tests/vault_unlock_across_restart_acceptance.rs` | a restart plus a refresh presenting the unlock key reopening the vault with no passphrase, and PR status performing; the key rotating; another user's key refused; a key that no longer opens still refreshing with `""`; a refresh that cannot read the vault handing the presented key back; no GitHub token in a refresh or device-login response; two tabs refreshing with one key at once |
| `tests/credential_vault_guard_acceptance.rs` | a create or reset refused without a fresh sign-in's waiting token; a refresh token refused where an access token belongs; a passphrase outside the accepted lengths refused; a reset refused while open; a key-less logout dropping the waiting token; wrong passphrases throttled, with the retry-after named (the set-aside cap is `tddy-credentials`' `sessions.rs` tests) |
| `tests/pending_login_expiry_acceptance.rs` | past `pending_login_ttl_seconds`, a first passphrase or a reset refused with *sign in to GitHub again* and the state unchanged; the sweep dropping an untouched token; no sweep at `0`; the startup, hold, expiry and refusal logs, none carrying a token or passphrase |
| `tests/open_vault_idle_expiry_acceptance.rs` | past `open_vault_idle_ttl_seconds` unused, PR status unavailable exactly as after a restart, and a refresh with the unlock key reopening the vault; a PR-status read and a refresh each keeping it open; `0` never closing it; the sweep closing a vault nobody looked at, and its period across both lifetimes; the startup `info` / `warn` and the closing log (login and idle seconds, no secret). Log capture is `tests/support/captured_log.rs` |
| `tests/vault_lifetime_config_acceptance.rs` | from a yaml config through `build_auth_entries`: both lifetimes' defaults (600 s, `REFRESH_TOKEN_TTL`), `0` as never, explicit values taken; a value past a week refusing to start naming the setting and `604800`, with and without `auth_storage`; a negative value refused by the config load, naming the setting |
| `tests/pr_lookup_credentials_acceptance.rs` | `pr_lookup_for_caller`'s `Empty`, `Unavailable(reason)` and `Perform(token)` |

## Related

- [codex-oauth-relay.md](./codex-oauth-relay.md) — authorize-URL validation and callback parsing
- [oauth-loopback-tunnel.md](./oauth-loopback-tunnel.md) — the operator TCP + `StreamBytes` bridge
- [`tddy-daemon-livekit`](../../tddy-daemon-livekit/docs/livekit-service.md) — mints room JWTs through a port, and carries each daemon's advertised key as opaque strings
- [`tddy-github` session tokens](../../tddy-github/docs/session-token.md) — the `v2` format these keys sign
- [`tddy-github` device flow](../../tddy-github/docs/device-flow.md) — the provider seam, both sign-in flows and `LoginAdmission`
- [`tddy-rpc` request transport](../../tddy-rpc/docs/request-transport.md) — the stamp `FirstLoginEnrolment` reads
- [`daemon-endpoint.md`](../../tddy-daemon/docs/daemon-endpoint.md) — `CommonRoomKeyDirectory`, the fleet's `KeyDirectory`
- [`tddy-daemon-kernel`](../../tddy-daemon-kernel/docs/daemon-kernel.md) — where `SessionUserResolver` is defined
- [`connection-service.md`](../../tddy-daemon/docs/connection-service.md) — what authenticates with the resolver
- [Codex OAuth relay (product)](../../../docs/ft/daemon/codex-oauth-relay.md)
- [changesets/](./changesets/)
