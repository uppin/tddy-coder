# Peer forwarding through an extension port - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 2/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [LiveKit peer discovery](../livekit-peer-discovery.md). One daemon forwards
  another daemon's RPC over the common room. The behaviour is unchanged, but the transport becomes
  a plugin behind a port.
- **Related Feature**: [Host and worktree services](../host-worktree-services.md). The host service
  forwards to peers through the same port.

## Summary

The daemon kernel stops depending on LiveKit. Forwarding an RPC to a peer daemon goes through a
`PeerForwarder` extension port. The kernel's `CommonRoom` keeps the forwarding *policy* (the
deadline, the stream idle timeout, the error a caller sees), and the LiveKit plugin supplies the
*transport*. A daemon with no LiveKit gets `NoPeerForwarding`: no peer is connected, and a forward is
refused with a precondition failure, as it is today when the common room is down.

## Background

`tddy-daemon-kernel::peer_forwarding` is the kernel's only LiveKit consumer, and about 20 crates
inherit the SDK through it. That includes six that use none of it. The open code issue
`packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md` records
that this already forced a module split elsewhere (`login_shell` and `pty_runtime`). The umbrella PRD
names this node as the largest single reduction of the LiveKit footprint.

## Proposed Changes

### What's Changing
- New port `PeerForwarder` in `tddy-daemon-extensions` (`is_connected`, `call_unary`,
  `call_server_stream`), with the null object `NoPeerForwarding`.
- `CommonRoom` is built from an `Arc<dyn PeerForwarder>`. `CommonRoom::from_config(slot, config)`
  becomes `CommonRoom::new(forwarder, config)`, `slot()` becomes `is_connected()`, and its forwarding
  methods keep their signatures.
- `LiveKitPeerForwarder` in `tddy-daemon-livekit` implements the port over the common-room `Room`
  slot, keeping each stream's client alive for the stream's whole life.
- `tddy-host-service::with_common_room` takes a `CommonRoom`, not a LiveKit `Room` slot.
- `tddy-daemon-kernel` and `tddy-host-service` drop `livekit` / `tddy-livekit` and leave the
  ratchet's `KNOWN_OFFENDERS`.

### What's Staying the Same
- `PeerRoute`, `classify_peer_route`, `daemon_rpc_identity`, `PEER_FORWARD_TIMEOUT` and
  `PEER_FORWARD_STREAM_IDLE_TIMEOUT` stay in the kernel unchanged. They never touched the SDK.
- Every error a forwarding caller sees: the deadline message, the "common room is not connected"
  precondition, and the mid-stream terminal error frame.
- Who forwards what. The forwarding consumers move in later nodes (10 and 11).

## Impact Analysis

### Technical Impact
- Kernel: `peer_forwarding.rs` loses its SDK import and gains a dependency on
  `tddy-daemon-extensions`.
- Plugin: new `LiveKitPeerForwarder`. `peer_routing.rs` and `livekit_peer_discovery.rs` build a
  `CommonRoom` through it.
- Call sites that construct a `CommonRoom` or read its slot: `tddy-daemon/src/runtime.rs:1571`, the
  host service, `tddy-session-agents/src/svc_provision_agent_clone.rs:195`,
  `tddy-session-split/src/svc_paired_codebase_teardown.rs:136`, and three plugin tests.
- `tddy-tools --no-default-features` stops carrying the SDK through the kernel, which reopens the
  `login_shell` / `pty_runtime` split that the code issue records.

### User Impact
None. The behaviour is identical.

## Implementation Plan
1. Add the port and its null object, with unit tests.
2. Make `CommonRoom` hold the port and delegate to it, with tests driven by an in-memory forwarder.
3. Add `LiveKitPeerForwarder` and repoint the constructors.
4. Remove the manifests' LiveKit edges and strike the two crates from the ratchet.

## Success Criteria
- `tddy-daemon-kernel` and `tddy-host-service` name no LiveKit crate, and the ratchet agrees.
- Every existing peer-forwarding test in `tddy-daemon-livekit` passes unchanged in substance.
- `CommonRoom`'s deadline and idle behaviour is pinned by tests that need no LiveKit server.

## Out of Scope
- Moving the forwarding consumers (nodes 10 and 11).
- A client-side deadline for LiveKit RPC (`docs/dev/todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`).
