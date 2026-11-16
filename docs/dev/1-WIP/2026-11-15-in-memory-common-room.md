# Changeset: An in-memory common room for the logic suites

**Date**: 2026-11-15
**Status**: 🚧 In Progress
**Type**: Refactor

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

`tddy-daemon`'s cross-host logic suites prove routing and forwarding behaviour, but they can only
run over a real LiveKit room, because common-room discovery and participant serving have no
transport-neutral seam. This node builds that seam for the discovery surface:

1. `CommonRoomPeerRegistry` learns a transport-neutral snapshot entry,
   `sync_from_participant_fields`, reusing the `peer_daemon_from_participant_fields` classifier
   that `sync_from_room` already delegates to. The LiveKit path is unchanged.
2. A new test crate, `tddy-room-testkit`, provides an in-memory room hub: participants join named
   rooms under an identity with metadata, watchers follow joins, leaves and metadata changes,
   registries attach to a room and stay synced, services are served under a participant identity
   and called through an `RpcClientTransport` (and through a `PeerForwarder` addressed the way the
   kernel's `CommonRoom` addresses peers), and topics carry broadcasts. Forwarded calls arrive
   stamped `RequestTransport::LiveKit` with the caller's identity, exactly as a real room stamps
   them, because services decide trust on that stamp.
3. The first logic suites run over it. Suites that need the session-room hosting surface or a full
   `runtime::build` stay on LiveKit in this node and are named in the placement test.

The hub is test support only: it lives in its own crate, depended on from `[dev-dependencies]`,
and no production crate names it.

## Background

Measured in `docs/dev/1-WIP/2026-11-15-livekit-compat-tests-initial-discovery.md` § *The remaining
15, by production surface*: discovery 9 suites, session-room 7, participant pair 6, raw `Room` 6,
full runtime 4. This node does the discovery surface and the participant pair it needs; the
session-room hosting port and the `RuntimeOptions` discovery seam are the following nodes.

## Scope

- [x] **Port**: `sync_from_participant_fields` on the registry, LiveKit path unchanged
- [x] **Hub**: `tddy-room-testkit` with join/metadata/watch, registry attach, RPC serve/call,
      `PeerForwarder`, broadcast; its own unit suites green without Docker
- [x] **Conversions**: first discovery suites run over the hub; placement test updated
- [x] **Baseline**: scoped runs back to recorded numbers, failing names unchanged
- [x] **Code Quality**: clippy and fmt clean on touched packages

Verified 2026-11-15: hub 4, relay 4, attach 8, multi-host 6, staging 2, daemon `test_placement` 4,
compat `livekit_test_placement` 3 — all green; clippy clean on `tddy-room-testkit`,
`tddy-daemon-livekit`, `tddy-livekit-compat-tests`, `tddy-daemon` (`--all-targets -D warnings`);
`cargo fmt --all --check` clean; CI script tests 33 green over both filtersets.

## Progress

| Piece | Result |
|---|---|
| `tddy-room-testkit` hub suites (`hub_room`, 4 tests) | green, 0.05 s, no Docker |
| `tddy-daemon` `relay_e2e_acceptance` (4) | converted, green in 0.48 s |
| `tddy-daemon` `session_attach_cross_host_acceptance` (8) | converted, incl. silenced-peer deadline |
| `tddy-daemon` `multi_host_acceptance` (6) | converted |
| `tddy-daemon` `staging_forwarding_acceptance` (2) | converted |

The hub grew one model the attach suite's deadline test needs: a **silenced** participant, in the
room and discovered, whose calls never return — the caller's forward deadline ends them, as against
a real room whose serving loop died.

**Not converted here, deliberately:** `tddy-worktree-service/remote_git_livekit_acceptance`. The
production `tddy-remote-git-repo` shim itself joins a LiveKit room after minting a token over HTTP,
so that suite tests a LiveKit product feature (git over LiveKit), not logic over a transport. It
stays on a real server; converting it would need a transport seam in the shipped shim binary, a
different decision. It remains in the placement test's remaining set with the session-room suites
(7) and the full-runtime suites (4), which are the following nodes.

## Decisions & Trade-offs

- **The hub stamps `LiveKit`, deliberately.** It models the LiveKit adapter's contract (that is
  what the compat package proves against a real server), so a logic suite converted onto it keeps
  asserting against the stamp its services will see in production. A suite that needs to prove the
  *cheap* adapters' stamps uses those adapters directly.
- **No fake `livekit::Room`.** Suites that join a raw SDK room keep doing so against a real server
  in this node; the hub offers named participants, which is what the converted suites consume.
