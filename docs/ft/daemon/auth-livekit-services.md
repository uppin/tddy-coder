# The identity boundary and the LiveKit service

**Product area:** daemon
**Status:** Active
**Updated:** 2026-09-09

## Summary

Two of the daemon's subsystems are crates of their own, each serving its gRPC services directly:

| Service | Methods | Served from |
|---|---|---|
| `auth.AuthService` | 5 | [`packages/tddy-daemon-auth`](../../../packages/tddy-daemon-auth/docs/auth-service.md) |
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

## One secret signs two things

`config.livekit.api_secret` signs **both** LiveKit room JWTs and session tokens, through
`tddy_github::SessionTokenSigner`. Session tokens are stateless and HMAC-signed on that key, which
is precisely how [cross-daemon session authentication](session-auth.md) works: a token minted by one
daemon is verifiable by every daemon holding the same secret, with no session store and no
propagation.

Auth and LiveKit being separate crates does **not** separate that secret, and **neither crate
derives its own**. A second signer would silently partition which tokens each half accepts, and the
partition would stay invisible until a cross-daemon call failed. The rule is held structurally
rather than by convention: `tddy-daemon-livekit` reaches minting through a `SessionTokenMinter`
**port**, and a test walks its manifest closure to prove `tddy-daemon-auth` is not on its dependency
path — so the port cannot quietly stop being one.

The same test, on both crates, proves `tddy-daemon` is absent from either dependency path. That is
what makes each extraction real rather than a re-export.

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

- [`packages/tddy-daemon-auth/docs/auth-service.md`](../../../packages/tddy-daemon-auth/docs/auth-service.md) — the identity boundary, the shared secret, and secrets at rest
- [`packages/tddy-daemon-livekit/docs/livekit-service.md`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md) — the rooms stream and the cut edges
- [`packages/tddy-daemon/docs/connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md) — the other 72 methods
- [Host and worktree services](host-worktree-services.md) — why the daemon is split at all
- [Cross-daemon session authentication](session-auth.md) — what the shared secret buys
- [Codex OAuth relay](codex-oauth-relay.md) — the OAuth surfaces the identity crate serves
- [LiveKit rooms panel](../web/livekit-rooms-panel.md) — the web client of `livekit.LiveKitService`
