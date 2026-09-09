# PRD: The auth and LiveKit subsystems in their own crates

**Date**: 2026-09-09
**PRD Type**: Architecture Change (breaking RPC change — 1 method)
**Product Area**: daemon
**Stack**: `#unbundle` node 4 of 8

## Affected Features

- [session-auth.md](../session-auth.md) — the identity boundary moves to its own crate
- [livekit-peer-discovery.md](../livekit-peer-discovery.md) — peer discovery and the common room move
- [session-room.md](../session-room.md) — the per-worktree LiveKit room moves
- [codex-oauth-relay.md](../codex-oauth-relay.md), [oauth-loopback-tunnel.md](../../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md) — move with auth
- [livekit-rooms-panel.md](../../web/livekit-rooms-panel.md) — **`StreamLiveKitRooms` changes coordinate**, so the panel's client moves
- [rpc-playground.md](../rpc-playground.md) — the service picker gains `livekit.LiveKitService`

## Summary

Two subsystems leave `tddy-daemon`: the identity boundary (`tddy-daemon-auth`, 2,145 prod LoC) and
everything LiveKit (`tddy-daemon-livekit`, 6,642 prod LoC across 5 modules with 6,898 LoC of dedicated
tests). Auth's four proto services are already separate, so it moves without any protocol change.
LiveKit takes **one** method with it — family T, `StreamLiveKitRooms` — which becomes
`livekit.LiveKitService`, and `packages/tddy-web`'s rooms panel migrates to the new coordinate.

## Background

These two are grouped because they are the last two daemon subsystems that can move without touching
`connection.ConnectionService`'s session families, and because they are entangled with each other in
exactly one place worth naming: **`config.livekit.api_secret` is the single secret that signs both
LiveKit room JWTs and session tokens**, via `tddy_github::SessionTokenSigner`. A reviewer of either
crate alone would not see that; a reviewer of one node sees it once.

The auth subsystem is a striking case of weak coupling behind a large line count. Its **entire**
dependency on the 23,000-line god module is one type alias — `auth.rs:25: use
crate::connection_service::SessionUserResolver;`. Once node 1 moved that alias into
`tddy-daemon-kernel`, auth became free-standing.

The LiveKit subsystem is the opposite: 6,642 lines with real edges in four directions, three of them
cycles that node 1 had to cut before this node could exist at all —
`livekit_peer_discovery ⇄ multi_host` (resolved by putting both in this crate),
`common_room_supervisor → daemon_config_service → livekit_peer_discovery` (spanning this crate and the
wiring layer), and `livekit_peer_discovery.rs:529`'s reach into
`crate::split_session::SPLIT_AGENT_IDENTITY_PREFIX` — a single `&str` constant that would otherwise
make the LiveKit crate depend on the session subsystem.

## Proposed Changes

### What changes

**`tddy-daemon-auth`** takes `auth.rs`, `github_token_store.rs`, `codex_oauth_relay.rs`,
`oauth_loopback_tunnel.rs`, `codex_oauth_participant_metadata.rs`, `github_pr_credentials.rs` and
`token_provider.rs`. It serves `auth.AuthService` (5 methods), `auth.LiveKitTokenService` (1),
`token.TokenService` (2) and `loopback_tunnel.LoopbackTunnelService` (1) — **every one already its own
proto**, so no coordinate moves and no client migrates for auth.

It exports `build_auth_entries(...) -> AuthBuildResult`, whose `user_resolver` is what every other
service authenticates with. That makes this crate the daemon's identity boundary in name as well as
in fact.

**`tddy-daemon-livekit`** takes `session_room.rs`, `livekit_peer_discovery.rs`,
`common_room_supervisor.rs`, `livekit_rooms_stream.rs` and `multi_host.rs`. It keeps the four trait
ports `session_room.rs` already defines — `SessionTerminalBridge`, `WorktreeSource`,
`SessionTokenMinter`, `RemoteSnapshotSource` — two of which `ConnectionServiceImpl` implements. That
direction is already correct for extraction: the god object depends on the LiveKit subsystem's traits,
not the reverse.

**Family T moves.** `StreamLiveKitRooms` leaves `connection.ConnectionService` for
`livekit.LiveKitService`, importing the shared `types.proto` node 1 introduced.

### What stays the same

- Every auth and LiveKit behaviour. `restructure verify --against <ref>` proves the statement multiset
  is unchanged.
- `daemon_settings.rs` and `daemon_config_service.rs` stay in the daemon — they serve the daemon's own
  configuration, which is wiring.
- The remaining 72 `ConnectionService` methods, until nodes 6–8.

## Impact Analysis

### Technical

| Area | Impact |
|---|---|
| `tddy-daemon` | −12 modules, −8,787 prod LoC; two `ServiceEntry` groups move behind constructors; the common-room supervisor task moves out of `RuntimeTasks` |
| `tddy-telegram` | its dependency on `session_room` **repoints** from `tddy-daemon` to `tddy-daemon-livekit`. This is why node 4 depends on node 2 |
| `tddy-service` | `livekit.proto` appears; `connection.proto` loses 1 rpc |
| `tddy-web` | `src/rpc/useLiveKitRooms.ts:68` and the rooms panel migrate; the Cypress `liveKitRoomsBackend.ts` fake moves |
| `tddy-integration-tests` | its `tddy_daemon::codex_oauth_relay` dependency repoints to `tddy-daemon-auth` — the only thing it reaches the daemon for |
| CI | Docker-dependent LiveKit suites abort in harness setup without `/var/run/docker.sock`; they move with the crate and their skip behaviour must not change |

### User-facing

One RPC coordinate moves. A web bundle from before this node cannot reach the LiveKit rooms panel's
stream; everything else is unaffected.

## Implementation Plan

1. `tddy-daemon-auth` extracted; its four services answer from the new crate.
2. `tddy-integration-tests` repointed to it for `codex_oauth_relay`.
3. `livekit.proto` created with family T; `build.rs`, `lib.rs`, descriptor set wired.
4. `tddy-daemon-livekit` extracted, taking `multi_host.rs` with `livekit_peer_discovery.rs` so their
   mutual reference stays inside one crate.
5. `tddy-telegram`'s `session_room` dependency repointed.
6. `tddy-web`'s rooms panel migrated; the Cypress fake split.

## Acceptance Criteria

- [ ] `auth.AuthService`, `auth.LiveKitTokenService`, `token.TokenService` and
      `loopback_tunnel.LoopbackTunnelService` all answer from `tddy-daemon-auth`
- [ ] `build_auth_entries` returns a working `user_resolver` and every other service authenticates with it
- [ ] `tddy-daemon-auth` compiles with **no** dependency on `tddy-daemon`
- [ ] `livekit.LiveKitService` serves `StreamLiveKitRooms`; `connection.ConnectionService` no longer declares it
- [ ] `tddy-daemon-livekit` compiles with no dependency on `tddy-daemon` and no reach into
      `split_session` or `daemon_config_service`
- [ ] `tddy-telegram` depends on `tddy-daemon-livekit`, not `tddy-daemon`
- [ ] `tddy-integration-tests` reaches `codex_oauth_relay` through `tddy-daemon-auth`
- [ ] the LiveKit rooms panel loads through `livekit.LiveKitService`
- [ ] the Docker-dependent LiveKit suites skip exactly as they do today when the socket is absent
- [ ] `./test -p tddy-daemon -p tddy-daemon-auth -p tddy-daemon-livekit` matches the recorded baseline

## References

- Changeset: [2026-09-09-unbundle-auth-livekit.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-auth-livekit.md)
- Discovery: [2026-09-09-unbundle-auth-livekit-initial-discovery.md](../../../docs/dev/1-WIP/2026-09-09-unbundle-auth-livekit-initial-discovery.md)
- Node 1's PRD: [PRD-2026-09-09-host-worktree-services.md](./PRD-2026-09-09-host-worktree-services.md)
