# Forwarding consumers off the LiveKit plugin - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 10/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [LiveKit peer discovery](../livekit-peer-discovery.md). The RPCs a daemon
  forwards to a peer (execute-tool, project mutations, host-document reads) behave the same, but their
  typed helpers stop living in the LiveKit plugin.
- **Related Feature**: [Codex OAuth relay](../codex-oauth-relay.md). The operator-side OAuth loopback
  tunnel over the common room moves into the plugin, which is the only place that has a room.

## Summary

The typed peer-forwarding helpers (`forward_*_via_livekit`) are thin encode and decode wrappers over
`CommonRoom`, which #624 made transport-neutral. They move into the kernel with transport-neutral
names. `PeerRouting` (the request router built from a `CommonRoom` and the eligible daemon source)
moves to `tddy-host-service`. `tddy-daemon-rpc` and `tddy-session-files` import them from the kernel. Daemon-auth's
OAuth loopback tunnel watches room participants and pipes bytes to one over a room RPC; that is
LiveKit work, so it moves into `tddy-daemon-livekit`, and the daemon's runtime starts it from there.
`tddy-daemon-rpc`, `tddy-session-files` and `tddy-daemon-auth` leave the ratchet's
`KNOWN_OFFENDERS`.

## Background

After #624 the kernel's `CommonRoom` forwards through a `PeerForwarder` port. The thirteen typed
helpers (`livekit_peer_discovery.rs:1363-1660`) and `PeerRouting` (`peer_routing.rs`) still live in
the plugin, so every consumer names the plugin to call them. Daemon-auth's last LiveKit use after
#626 is `oauth_loopback_tunnel.rs` (`livekit::prelude::{Room, RoomEvent}`, `tddy_livekit::RpcClient`).

## Proposed Changes

### What's Changing
- The kernel gains `peer_calls` (typed helpers: `forward_add_project_to_host`, …,
  `forward_stream_read_host_document`, and the session ones node 11 will use).
  `tddy-host-service` gains `peer_routing` (`PeerRouting::new(config, eligible,
  Option<CommonRoom>)`) beside the `EligibleDaemonSource` it needs. The kernel cannot take it,
  because `tddy-host-service` depends on the kernel.
- The plugin keeps `forward_*_via_livekit` as `#[deprecated]` re-exports of the kernel helpers until
  node 11 removes the last callers.
- `tddy-daemon-rpc` and `tddy-session-files` import the kernel's names.
- `oauth_loopback_tunnel.rs` moves to `tddy-daemon-livekit::oauth_loopback_tunnel`, and the runtime
  calls it there. Daemon-auth keeps the pure `codex_oauth_participant_metadata` parsing it uses.
- Three crates are struck from the ratchet.

### What's Staying the Same
- Every forwarded RPC's service and method, deadline and error.
- The tunnel's behaviour: eligibility config, port resolution and bridging.

## Impact Analysis
- **Technical:** helpers move and are renamed; `PeerRouting`'s constructor takes a `CommonRoom`
  rather than a `Room` slot; one module moves between crates.
- **User:** none.

## Implementation Plan
1. Kernel `peer_calls` and host-service `peer_routing` (moved), with the plugin's deprecated re-exports.
2. Repoint daemon-rpc and session-files.
3. Move the tunnel and repoint the runtime. Trim the manifests and strike the ratchet entries.

## Success Criteria
- The three crates name no LiveKit crate, and the ratchet agrees.
- The typed helpers are tested against an in-memory `PeerForwarder`.
- The existing forwarding and tunnel suites still pass.

## Out of Scope
- The session-agents, session-split, lifecycle and launch consumers (node 11).
