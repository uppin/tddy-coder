# The identity boundary (tddy-daemon-auth)

Who a session token belongs to, and every credential the daemon holds on that person's behalf —
served as four gRPC services and one function.

`AuthBuildResult::user_resolver` is the daemon's single identity function. Every other service, in
every other crate, authenticates with a clone of it. That is what makes this crate the identity
boundary in fact and not only in name.

## Where the code lives

| Module | What is in it |
|---|---|
| `auth` | `build_auth_entries`, the `auth.AuthService` and `auth.LiveKitTokenService` handlers, `session_token_authenticator`, `build_token_service_entry` |
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

## One secret signs two things

`config.livekit.api_secret` signs **both** LiveKit room JWTs and session tokens, through
`tddy_github::SessionTokenSigner`. Session tokens are stateless and HMAC-signed on that key, so a
token minted by one daemon is verifiable by every daemon holding the same secret — which is the
whole of how a deployment shares an identity.

Splitting auth from LiveKit into two crates does **not** split that secret, and **neither crate may
start deriving its own**. A second signer would silently partition which tokens each half accepts,
and the partition would be invisible until a cross-daemon call failed. The enforcement is
structural rather than a comment: `tddy-daemon-livekit` reaches minting through a
`SessionTokenMinter` **port**, and `tests/dependency_boundary_unit.rs` on that side pins
`tddy-daemon-auth` off its dependency path, so the LiveKit crate cannot grow a signer by accident.

When no secret is configured the daemon still starts, but auth is non-functional: minting fails and
the resolver rejects every token. `build_auth_entries` answers a daemon with no `github:` block with
`user_resolver: None`, and the wiring layer registers **no session services at all** — a refusal,
not a permissive default.

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

1. An **existing** storage directory no longer has `0o700` re-imposed on every write, so the daemon
   stops overruling an operator's deliberate `chmod`. The cost: an `auth_storage` that is currently
   group- or world-readable **stays that way**, where before every `put` re-tightened it.
2. The mode applies to **every** directory the call creates, not just the leaf. With
   `auth_storage = /var/lib/tddy/auth` and no `/var/lib/tddy`, that parent is created `0700` and
   owned by the daemon user; it previously took the process umask.

An unwritable `auth_storage` fails `build_auth_entries` outright. Retention is a hard login
dependency — a failed `put` fails the exchange — so an unwritable path breaks *every* login rather
than merely degrading PR status, and `install` only creates and chowns `/var/lib/tddy` on the
root/systemd path, which makes this a reachable misconfiguration rather than a theoretical one.

## Log targets still name `tddy_daemon`

`tddy_daemon::auth`, `tddy_daemon::codex_oauth`, `tddy_daemon::github_token_store` and
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
| `tests/auth_service_acceptance.rs` | all five `auth.AuthService` methods answering from this crate, plus a foreign-secret refusal |
| `tests/token_service_acceptance.rs` | `MintLiveKitToken` and `token.TokenService` minting against `config.livekit.api_secret`, each verified with `livekit_api::access_token::TokenVerifier` — and a server holding a *different* secret refusing the same JWT, without which the positive cases assert nothing |
| `tests/cross_crate_session_token_acceptance.rs` | a token signed here authenticating a call to a service in another crate. The far side is `tddy-service` deliberately: this crate cannot reach the daemon's own services, which is exactly the property below |
| `tests/dependency_boundary_unit.rs` | `tddy-daemon` is absent from this crate's transitive manifest closure — with a third test asserting the walk actually reaches `tddy-daemon-kernel`, so a walk that silently found nothing cannot pass as a clean result |
| `github_token_store.rs` (inline) | a failure part-way through a write leaves the previous secret intact |

⚠ **The atomic-write test does not discriminate the change that motivated it.** The base
implementation already staged to `<tokens>.tmp` and renamed, and that staged create also fails in a
`0o555` directory — so reverting the `write_atomic_with_mode` refactor would leave the test green.
It is an honest guard against a *future* truncate-in-place, not evidence that the refactor happened.

## Related

- [codex-oauth-relay.md](./codex-oauth-relay.md) — authorize-URL validation and callback parsing
- [oauth-loopback-tunnel.md](./oauth-loopback-tunnel.md) — the operator TCP + `StreamBytes` bridge
- [`tddy-daemon-livekit`](../../tddy-daemon-livekit/docs/livekit-service.md) — the other half of the one shared secret
- [`tddy-daemon-kernel`](../../tddy-daemon-kernel/docs/daemon-kernel.md) — where `SessionUserResolver` is defined
- [`connection-service.md`](../../tddy-daemon/docs/connection-service.md) — what authenticates with the resolver
- [Codex OAuth relay (product)](../../../docs/ft/daemon/codex-oauth-relay.md)
- [changesets/](./changesets/)
