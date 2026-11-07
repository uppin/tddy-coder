# LiveKit wiring package (`tddy-livekit-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 6/16 — branch `feature/web-split/livekit`

## Affected Features

- **Primary**: [livekit-rooms-panel.md](../livekit-rooms-panel.md),
  [daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md),
  [screen-sharing-sessions.md](../screen-sharing-sessions.md), [vnc-sessions.md](../vnc-sessions.md),
  [livekit-participant-owned-projects.md](../livekit-participant-owned-projects.md).
- **Related**: chat, session usage and PR-stack chat, which use LiveKit rooms today.

## Summary

Create a new `packages/tddy-livekit-web` that owns **every remaining use of `livekit-client`** in the
web app, and put the features that use LiveKit behind ports:

- the LiveKit **connection provider** (implements `ConnectionProvider` from `tddy-client-web`),
  the common room, the session room connection and the room terminal feed;
- **presence** — host presence and the presence room (`PresencePort`);
- **room participants and media** — participant lists and camera video, screen-sharing and VNC media
  tracks and input forwarding, the presenter room (`MediaPort`);
- the LiveKit screens (`LiveKitRoomsPanel`, `ParticipantList`, the participant video dialog).

After this node the rule holds: **only `tddy-livekit-web` and `tddy-livekit-rpc-web` import
`livekit-client`.** Feature code still in `tddy-web` (chat, sessions, prstack) uses the ports, so the
packages extracted next can move without dragging LiveKit with them. `tddy-web` wires LiveKit in.

## Background

31 files in `tddy-web` import `livekit-client` directly, spread over chat, sessions, hooks, rpc and
lib. A component package that imports one of them depends on LiveKit, which the developer ruled out.

## Proposed Changes

### What's Changing

- New package; `PresencePort` and `MediaPort` declared beside `ClientsPort` in `tddy-client-web`;
  LiveKit implementations here; the LiveKit Cypress support (`fakeCommonRoom`, the recording LiveKit
  RPC, `withLiveCommonRoom`, the Docker testkit for tests that need a server) moves here.
- Feature code switches from LiveKit objects to the ports.
- A workspace-wide rule test: no file outside the two LiveKit packages imports `livekit-client`.

### What's Staying the Same

- Every LiveKit-backed behaviour — RPC over rooms, presence, screen sharing, VNC, camera, the rooms
  panel — works exactly as before. A daemon with LiveKit off behaves as before.

## Impact Analysis

### Technical Impact

- The broadest node of the stack: ports for presence and media are new contracts. If the media port
  proves too wide, the node splits by capability (presence + RPC first, media second) — see the changeset.

### User Impact

None.

## Acceptance Criteria

- [ ] Only the two LiveKit packages import `livekit-client` (rule test).
- [ ] LiveKit specs (rooms panel, presence, screen sharing, VNC, participant video, common room) pass
      from `tddy-livekit-web`'s own Cypress config.
- [ ] Feature specs in `tddy-web` that used LiveKit pass with port doubles.
- [ ] The app behaves as before with and without LiveKit configured.

## References

- Predecessors (real): `livekit-rpc` (transport at its new name), `clients` (ports), `ui`.
