# LiveKit media behind the media port - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 11/16 — branch `feature/web-split/media`

## Affected Features

- **Primary**: [screen-sharing-sessions.md](../screen-sharing-sessions.md), [vnc-sessions.md](../vnc-sessions.md), participant camera video ([livekit-participant-owned-projects.md](../livekit-participant-owned-projects.md)).
- **Related**: PR-stack and workflow chat (presenter room), hosts (screen-sharing/VNC overlays).

## Summary

Move the LiveKit **media** layer into `tddy-livekit-web` behind `MediaPort`: the screen-sharing and VNC overlays' track handling and input forwarding (`useScreenSharingInput`), the presenter room (`usePresenterLiveKitRoom`), participant camera video and the participant video preview. Feature components keep their layout and take an opaque `RoomHandle`; rendering a remote track goes through the port's `RemoteVideo`. After this node no feature file in `tddy-web` imports `livekit-client`.

## Background

Split out of `feature/web-split/livekit` when its implementation stopped on the boundary its own changeset predicted: the media components own LiveKit tracks directly (`RoomEvent`, `VideoTrack.attach`), so hiding them behind a port reshapes the screen-sharing, VNC and inspector tabs — a reviewable change of its own, separate from moving the connection and presence layer.

## Proposed Changes

### What's Changing

- `MediaPort` (final shape) and its LiveKit implementation in `tddy-livekit-web`; the double in `tddy-client-web/testkit`.
- Screen-sharing, VNC, presenter-room and camera specs use the media double or move to `tddy-livekit-web`.
- The LiveKit import rule (`scripts/livekit-import-boundary.test.ts`) goes green here.

### What's Staying the Same

- Screen sharing, VNC input, presenter room and participant video behave as before.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] No file outside the two LiveKit packages imports `livekit-client` (rule test green).
- [ ] Media specs pass with the media double or from `tddy-livekit-web`.
- [ ] Hosts and sessions render screen sharing and VNC unchanged.

## References

- Predecessor (real): `livekit` (the package, `RoomHandle`, presence and room-RPC ports). Successors: `hosts` (overlays), `session` (media tabs).
