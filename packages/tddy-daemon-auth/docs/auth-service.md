# The identity boundary (tddy-daemon-auth)

Who a session token belongs to, and every credential the daemon holds on that person's behalf —
served as four gRPC services and one function.

`AuthBuildResult::user_resolver` is the daemon's single identity function. Every other service, in
every other crate, authenticates with a clone of it. That is what makes this crate the identity
boundary in fact and not only in name.

## Where the code lives

| Module | What is in it |
|---|---|
| `auth` | `build_auth_entries_with` (a daemon with a signing identity) and `build_auth_entries` (one without), the `auth.AuthService` and `auth.LiveKitTokenService` handlers, `session_token_authenticator`, `build_token_service_entry` |
| `signing_key` | `DaemonSigningKey`, `load_signing_key` / `signing_key_path`, the `KeyDirectory` port and `StandaloneKeyDirectory`, `DirectorySessionTokenVerifier`, `SessionTokens`, `auth_storage_looser_than_owner_only` |
| `github_token_store` | `FileGitHubTokenStore` — where a login's GitHub access token sits at rest |
| `github_pr_credentials` | the credential shape `ConnectionServiceImpl` reads a PR list with |
| `codex_oauth_relay` | authorize-URL validation and callback parsing — [codex-oauth-relay.md](./codex-oauth-relay.md) |
| `oauth_loopback_tunnel` | the operator-side TCP listener and its LiveKit bridge — [oauth-loopback-tunnel.md](./oauth-loopback-tunnel.md) |
| `codex_oauth_participant_metadata` | the `codex_oauth` participant-metadata shape both halves read |
| `token_provider` | the token-source seam a caller injects |

## Services

| Service | Methods | Notes |
|---|---|---|
| `auth.AuthService` | 5 | the GitHub OAuth exchange and what it persists |
| `auth.LiveKitTokenService` | 1 | `MintLiveKitToken` — a room JWT |
| `token.TokenService` | 2 | session tokens |
| `loopback_tunnel.LoopbackTunnelService` | 1 | `StreamBytes`, the session-host end of the OAuth tunnel |

**No wire coordinate changed when this crate was cut out of `tddy-daemon`.** All four were already
their own protos, so no client migrated — which is why the identity boundary could be drawn early
without a breaking change riding along.

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
and handed to `build_auth_entries_with`, the local socket, `local_token.LocalTokenService` and the
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

Every write goes through `tddy_core::atomic_file::write_atomic_with_mode`, which stages to a swap
file and renames. A crash mid-write leaves the previous value intact; an empty secrets file reads as
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
   `0700` is instead **warned about once, at startup**, by `build_auth_entries_with`
   (`auth_storage_looser_than_owner_only`), and left as it is. A directory's mode governs listing
   and traversal, not the contents of the `0600` files inside it, so the signing key and the token
   store are protected by their own modes even in a loose directory.
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
under too), `tddy_daemon::codex_oauth`, `tddy_daemon::github_token_store` and
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
| `tests/auth_service_acceptance.rs` | all five `auth.AuthService` methods answering from this crate, plus a refusal of a token signed by a key this daemon does not know |
| `tests/token_service_acceptance.rs` | `MintLiveKitToken` and `token.TokenService` minting room JWTs against `config.livekit.api_secret` (its only job), each verified with `livekit_api::access_token::TokenVerifier` — and a server holding a *different* secret refusing the same JWT, without which the positive cases assert nothing |
| `tests/cross_crate_session_token_acceptance.rs` | a token signed here authenticating a call to a service in another crate. The far side is `tddy-service` deliberately: this crate cannot reach the daemon's own services, which is exactly the property below |
| `tests/per_daemon_signing_identity_acceptance.rs` | daemon A's token verifying on daemon B once B's directory holds A's key — B's directory is asked for exactly A's key id — and an unseen key id refused with no fallback |
| `tests/auth_without_livekit_acceptance.rs` | a daemon with no `livekit:` block completing a sign-in through the served `ExchangeCode`, and the token it returns resolving to that login |
| `tests/auth_storage_posture_warning_acceptance.rs` | exactly one startup warning for a `0755` `auth_storage`, none for `0700` |
| `signing_key.rs` (inline) | generate once at `0600` and reuse byte-identically across a restart; a group-readable key refused and left unrepaired; the key's location from `auth_storage` or `tddy_data_dir`, and a refusal with neither; a directory lookup that cannot answer on the first poll refused without waiting. The first-boot hard-link race has no dedicated test |
| `tests/dependency_boundary_unit.rs` | `tddy-daemon` is absent from this crate's transitive manifest closure — with a third test asserting the walk actually reaches `tddy-daemon-kernel`, so a walk that silently found nothing cannot pass as a clean result |
| `github_token_store.rs` (inline) | a failure part-way through a write leaves the previous secret intact |

⚠ **The atomic-write test does not discriminate the change that motivated it.** The base
implementation already staged to `<tokens>.tmp` and renamed, and that staged create also fails in a
`0o555` directory — so reverting the `write_atomic_with_mode` refactor would leave the test green.
It is an honest guard against a *future* truncate-in-place, not evidence that the refactor happened.

## Related

- [codex-oauth-relay.md](./codex-oauth-relay.md) — authorize-URL validation and callback parsing
- [oauth-loopback-tunnel.md](./oauth-loopback-tunnel.md) — the operator TCP + `StreamBytes` bridge
- [`tddy-daemon-livekit`](../../tddy-daemon-livekit/docs/livekit-service.md) — mints room JWTs through a port, and carries each daemon's advertised key as opaque strings
- [`tddy-github` session tokens](../../tddy-github/docs/session-token.md) — the `v2` format these keys sign
- [`daemon-endpoint.md`](../../tddy-daemon/docs/daemon-endpoint.md) — `CommonRoomKeyDirectory`, the fleet's `KeyDirectory`
- [`tddy-daemon-kernel`](../../tddy-daemon-kernel/docs/daemon-kernel.md) — where `SessionUserResolver` is defined
- [`connection-service.md`](../../tddy-daemon/docs/connection-service.md) — what authenticates with the resolver
- [Codex OAuth relay (product)](../../../docs/ft/daemon/codex-oauth-relay.md)
- [changesets/](./changesets/)
