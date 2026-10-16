# Client room transport through a port - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 7/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Session worktree sync](../session-worktree-sync.md). `tddy-session-sync`
  joins a session room and mirrors the worktree.
- **Related Features**: [Remote git repo](../remote-git-repo.md) (`tddy-remote-git-repo`, the git
  SSH shim over a room), [Terminal sessions](../terminal-sessions.md) (`tddy-tools pty-relay
  --livekit-url`), and the in-session tool client's LiveKit transport (`tddy-session-tool-client`,
  `tddy-discovery`).

## Summary

Client tools stop naming LiveKit in their libraries. Joining a room (minting the participant's
token, waiting for the serving participant), calling RPCs in it, holding an interactive bidi stream,
and subscribing to broadcast topics all go through a small client-side port crate,
`tddy-room-client`. That crate is built on `tddy_rpc::RpcClientTransport`, which already abstracts
the calls. The LiveKit implementation lives in `tddy-livekit`, and each client *binary*
(`tddy-tools`, `tddy-session-sync`, `tddy-remote-git-repo`) wires it. `tddy-session-sync`,
`tddy-remote-git-repo`, `tddy-terminal-rpc` and `tddy-session-tool-client` leave the ratchet's
`KNOWN_OFFENDERS`.

## Background

`tddy_livekit::client_connect::connect_client` already de-duplicated the join for three callers
([2026-08-15 TODO](../../../dev/todo/2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md)).
Each caller still mints a token from API credentials and holds `tddy_livekit` types in its library:
`ConnectedClient`, `RpcClient`, `BroadcastChannel`, `BidiStreamSender`, `livekit::Room`. The libraries
are the code under test, and today every such test links the SDK.

## Proposed Changes

### What's Changing
- New crate `tddy-room-client`, which reaches no LiveKit crate:
  - `RoomJoin`: URL, room, own identity, and how to authenticate (API credentials or a token).
  - `RoomConnector::connect(join, peer_identity, wait)`, returning `RoomSession`.
  - `RoomSession`: `rpc() -> Arc<dyn RpcClientTransport>`, `open_duplex(service, method)` (the live
    bidi stream), `subscribe(topic)`, `peer_present()`.
  - `RoomConnectError` (`Room` / `PeerAbsent`), mirroring today's `ConnectError`.
- `tddy-livekit` implements it (`LiveKitRoomConnector`) over today's `connect_client`,
  `RpcClient` and `BroadcastChannel`, and mints the token when it is given credentials.
- Libraries take `Arc<dyn RoomConnector>`: session-sync's `attach`, remote-git-repo's relay,
  terminal-rpc's pty-relay LiveKit mode, and tool-client's LiveKit transport (with its room cache).
- Binaries construct the LiveKit connector: `tddy-session-sync`, `tddy-remote-git-repo`, and
  `tddy-tools` behind its `livekit` feature. The `livekit` features on `tddy-terminal-rpc`,
  `tddy-session-tool-client` and `tddy-discovery` become unnecessary and are removed. `tddy-tools`
  keeps the feature, which now only selects whether its binary links the connector.
- Four crates are struck from the ratchet.

### What's Staying the Same
- Every wire behaviour: identities, room names, token TTLs, the wait for the serving participant,
  and the error wording for "room unreachable" and "daemon absent".
- The in-jail `tddy-tools --no-default-features` build carries no LiveKit, as today.

## Impact Analysis
- **Technical:** five library crates and three binaries. Library tests can now use an in-memory
  `RoomConnector`.
- **User:** none.

## Implementation Plan
1. Add the port crate with an in-memory test connector, then the plugin's `LiveKitRoomConnector`.
2. Switch the libraries one at a time and wire the binaries.
3. Remove the features, trim the manifests and strike the ratchet entries.

## Success Criteria
- The four libraries name no LiveKit crate, and the ratchet agrees.
- The existing testkit-backed suites (sync, remote git, pty-relay) pass through the binaries'
  wiring.
- Each library's join logic is tested against an in-memory connector.

## Out of Scope
- De-duplicating tool-client's three transport selections
  ([2026-10-01 TODO](../../../dev/todo/2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md)).
  Same file, different concern.
- `tddy-coder` (node 8).
