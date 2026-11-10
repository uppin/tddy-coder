# LiveKit wiring package (`tddy-livekit-web`)

**Package:** `packages/tddy-livekit-web`
**Applies to:** every web component package that uses a LiveKit room
**Added:** 2026-11-07 (#web-split 6/17, PR #843); media 2026-11-07 (#web-split 11/17, PR #855)

## Overview

A web component package must not depend on LiveKit. `tddy-livekit-web` owns the LiveKit side of the
web app — the connection to hosts over the common room, presence, and RPC addressed to a participant
in a room — and features reach it through ports declared in `tddy-client-web`:

- **Presence port** — which room answers for a host, the presence room in scope, who is in a room.
- **Room-RPC port** — a client for a service on a participant in a room, whether that participant is
  there (rendered, or asked at the moment of a send), and the room's connection state.
- **Media port** — the presenter room a session names, input forwarding to a screen-sharing or VNC
  bridge, whether a participant has camera video, and rendering a remote video track or a camera
  preview.

`tddy-web` registers the LiveKit implementations with one call. Rooms are opaque handles outside the
package.

## Behaviour

- Every LiveKit-backed behaviour — RPC over rooms, presence, the rooms panel and participant list,
  host connection over the common room — works as before. A daemon with LiveKit off behaves as before.
- Chat refuses a send when the presenter is not in the room, judged at the moment of the send.
- Only `tddy-livekit-web` and `tddy-livekit-rpc-web` import `livekit-client` (enforced by a workspace
  rule test, no exceptions).
- The roster's camera preview shows the participant's camera (it opened with no track and said
  "No camera track").

## Acceptance criteria

- [x] Only the two LiveKit packages import `livekit-client` (rule test).
- [x] Screen sharing, VNC, the presenter room and camera video work through the media port (their specs pass on the media double or the package).
- [x] The LiveKit specs whose subject lives in the package pass from its own Cypress config.
- [x] Feature specs in `tddy-web` that used LiveKit pass against the package's testkit.
- [x] The app behaves as before with and without LiveKit configured.

Technical reference: [packages/tddy-livekit-web/docs/README.md](../../../packages/tddy-livekit-web/docs/README.md).
