# LiveKit wiring package (`tddy-livekit-web`)

**Package:** `packages/tddy-livekit-web`
**Applies to:** every web component package that uses a LiveKit room
**Added:** 2026-11-07 (#web-split 6/17, PR #843); media follows in #855

## Overview

A web component package must not depend on LiveKit. `tddy-livekit-web` owns the LiveKit side of the
web app — the connection to hosts over the common room, presence, and RPC addressed to a participant
in a room — and features reach it through ports declared in `tddy-client-web`:

- **Presence port** — which room answers for a host, the presence room in scope, who is in a room.
- **Room-RPC port** — a client for a service on a participant in a room, whether that participant is
  there (rendered, or asked at the moment of a send), and the room's connection state.

`tddy-web` registers the LiveKit implementations with one call. Rooms are opaque handles outside the
package.

## Behaviour

- Every LiveKit-backed behaviour — RPC over rooms, presence, the rooms panel and participant list,
  host connection over the common room — works as before. A daemon with LiveKit off behaves as before.
- Chat refuses a send when the presenter is not in the room, judged at the moment of the send.
- Only `tddy-livekit-web` and `tddy-livekit-rpc-web` import `livekit-client`, except the media files
  that #855 moves (enforced by a workspace rule test with an explicit allowlist).

## Acceptance criteria

- [x] Only the two LiveKit packages import `livekit-client`, media files excepted until #855 (rule test).
- [x] The LiveKit specs whose subject lives in the package pass from its own Cypress config.
- [x] Feature specs in `tddy-web` that used LiveKit pass against the package's testkit.
- [x] The app behaves as before with and without LiveKit configured.

**Delivered elsewhere:** the media requirements of the original plan — screen sharing, VNC, the
presenter room and camera video behind `MediaPort`, and their specs — are `feature/web-split/media`
(#855).

Technical reference: [packages/tddy-livekit-web/docs/README.md](../../../packages/tddy-livekit-web/docs/README.md).
