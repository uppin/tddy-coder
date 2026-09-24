# The identity boundary and the LiveKit service

**Product area:** daemon
**Status:** Active
**Updated:** 2026-09-23

## Summary

Two of the daemon's subsystems are crates of their own, each serving its gRPC services directly:

| Service | Methods | Served from |
|---|---|---|
| `auth.AuthService` | 7 — the redirect flow, the device flow, status, refresh, logout | [`packages/tddy-daemon-auth`](../../../packages/tddy-daemon-auth/docs/auth-service.md) |
| `auth.LiveKitTokenService` | `MintLiveKitToken` | the same crate |
| `token.TokenService` | 2 | the same crate |
| `loopback_tunnel.LoopbackTunnelService` | `StreamBytes` | the same crate |
| `livekit.LiveKitService` | `StreamLiveKitRooms` | [`packages/tddy-daemon-livekit`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md) |

`tddy-daemon-auth` is the daemon's **identity boundary**: it holds who a session token belongs to,
and every credential the daemon keeps on that person's behalf. Its `build_auth_entries` returns the
`user_resolver` that **every other service, in every other crate, authenticates with** — one
identity function per daemon, so no two services can drift on who a caller is.

`tddy-daemon-livekit` holds everything the daemon does with LiveKit: the per-worktree
[session room](session-room.md), [common-room peer discovery](livekit-peer-discovery.md), the
supervisor that keeps the common room joined, and the rooms stream behind the web's
[LiveKit rooms panel](../web/livekit-rooms-panel.md).

## What a client sees

**One coordinate is different, and only one.** `StreamLiveKitRooms` is addressed as
`livekit.LiveKitService`. Everything else is where it was: auth's four services were always their
own protos, so nothing about a login, a token mint or the OAuth tunnel changed on the wire.

**Every transport carries the new service.** `livekit.LiveKitService` is a `ServiceEntry` registered
beside the others, so it reaches clients over Connect-HTTP `/rpc`, over the LiveKit common room and
over the local UDS socket. The [RPC Playground](rpc-playground.md) lists it without any change,
because it discovers services through gRPC ServerReflection rather than a compiled list.

**A web bundle and a daemon must be from the same side of that one method.** An older bundle asking
`connection.ConnectionService` for the rooms stream gets `unimplemented`; nothing else is affected.
This is the repo's standing policy — break freely, migrate every consumer in the same change.

## One key signs one thing

Each daemon signs session tokens with an **Ed25519 keypair of its own** (`DaemonSigningKey`,
generated on first boot into `auth_storage` and reused after), and `config.livekit.api_secret`
signs LiveKit room JWTs and nothing else. A token names its signer's key id, so
[cross-daemon session authentication](session-auth.md) needs no shared secret: a daemon verifies
its own tokens with its own key, and a peer's against the public key that peer advertises on the
common room. Neither the LiveKit credential nor its absence decides whether a daemon can
authenticate — a daemon with no `livekit:` block at all signs, verifies, and completes a sign-in.

Every signer and verifier in a daemon comes from its **one** `SessionTokens` value, built once in
`runtime::build` and handed to the login flow, the local socket's mint and the split-session agent
credentials. No path constructs a second key: that would be an identity no peer was told about,
and tokens signed with it would silently fail everywhere else.

**Key distribution is a port, so the LiveKit crate never reaches auth.** `tddy-daemon-auth`
declares the `KeyDirectory` trait ("what public key does key id `kid` name?") and its
`StandaloneKeyDirectory` for a daemon with no fleet. `tddy-daemon-livekit` carries a daemon's key
on its common-room advertisement as two **opaque strings** (`AdvertisedSigningKey`: the key id and
the base64url SPKI DER) and hands peers' strings back undecoded. The adapter between them,
`CommonRoomKeyDirectory`, lives in `tddy-daemon` — the one crate that depends on both. Room JWTs
cross the same boundary the other way, through the `SessionTokenMinter` port. The rule is held
structurally: a test walks `tddy-daemon-livekit`'s manifest closure to prove `tddy-daemon-auth` is
not on its dependency path, so neither port can quietly stop being one.

The same test, on both crates, proves `tddy-daemon` is absent from either dependency path. That is
what makes each extraction real rather than a re-export.

**Which participant's key is believed** is one predicate, `tddy_service::may_be_daemon_discovery_identity`,
read by both peer discovery and every client-facing LiveKit mint — see
[LiveKit peer discovery § Trust model](livekit-peer-discovery.md#trust-model).

## Where the seam is drawn

- **By the service the methods belong to, not by the subsystem's name.** The host-key path —
  `host_keypair`, `host_private_key`, `ssh_agent`, `ssh_agent_add` — is part of the
  [host service](host-worktree-services.md), because `AddHostKey` and `ListHostKeyCandidates` are
  host-service methods. So the identity boundary is smaller than "auth" suggests: **what is in it
  signs and verifies; what left with the host service unlocks and loads.**
- **Configuration is wiring, and stays.** `daemon_settings` and `daemon_config_service` serve the
  daemon's own settings rather than an operator's identity, so they are not part of either crate.
- **A cross-crate reach becomes a port, a lifted symbol, or a re-export — never a cycle.** Rust
  crates cannot be mutually dependent, so each edge crossing the new boundaries was cut in the
  direction that survives the *next* move as well: an identity constant belongs with the identity
  names several crates need, a filesystem convention belongs with the crate that already owns the
  directory layout, and a trait belongs beside its one implementation.
- **A proto is cut only where its messages allow it.** `livekit.proto` imports nothing: the twelve
  messages `StreamLiveKitRooms` reaches overlap with nothing that stays in `connection.proto`.

## Related documentation

- [`packages/tddy-daemon-auth/docs/auth-service.md`](../../../packages/tddy-daemon-auth/docs/auth-service.md) — the identity boundary, the signing key, and secrets at rest
- [`packages/tddy-daemon-livekit/docs/livekit-service.md`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md) — the rooms stream and the cut edges
- [`packages/tddy-daemon/docs/connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md) — the other 72 methods
- [Host and worktree services](host-worktree-services.md) — why the daemon is split at all
- [Cross-daemon session authentication](session-auth.md) — the token format and how a peer's token is verified
- [Codex OAuth relay](codex-oauth-relay.md) — the OAuth surfaces the identity crate serves
- [LiveKit rooms panel](../web/livekit-rooms-panel.md) — the web client of `livekit.LiveKitService`
