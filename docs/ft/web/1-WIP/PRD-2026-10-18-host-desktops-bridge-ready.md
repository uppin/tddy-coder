# Report a desktop's sign-in failure and its real size — PRD

**Date**: 2026-10-18
**PRD Type**: Bug Fix + Feature Enhancement
**Stack**: `#host-desktops` 6/8. Umbrella on `feature/host-desktops/service-split`.

## Affected Features

- [Screen sharing sessions](../screen-sharing-sessions.md): AC-SS-5, AC-SS-11; § Out of scope "Connection health / reconnect UX beyond initial error display"

## Summary

Starting a desktop, on either scope, returns only once the bridge has signed in and published its
track. The response carries the desktop's **real** framebuffer size. If the server refuses the
credentials, the start fails with `Unauthenticated` and the server's reason. If the bridge fails in
any other way, or never reports, the start fails with that failure or a timeout. Either way no bridge
is left running. The viewer shows that error instead of "Connecting…" forever, and scales pointer
input into the real size.

## Background

`spawn_bridge` returns once the JSON is written to the child's stdin. Authentication happens later,
inside the bridge, and a failure is only logged to inherited stderr. The browser then waits on a
track that never arrives. The reply hard-codes `1920×1080` (`DEFAULT_STREAM_WIDTH/HEIGHT`), while the
bridge publishes at the server's real size, so pointer input is mis-scaled on every other resolution.

## Proposed Changes

### What's Changing
- Bridge → daemon readiness protocol: one JSON line on the bridge's stdout, `{"ready":{"width":W,"height":H}}` after the track is published, or `{"failed":{"kind":"authentication"|"connect"|"livekit","message":…}}`. `tddy-screenshare` emits it; both bridges classify their authentication errors.
- `StartStream` / `StartHostStream` wait for that line (bounded), map `authentication` to `Unauthenticated` and others to `Unavailable`, and on failure or timeout terminate the child.
- The overlays (session and host) show the start error, and the host overlay passes the real size to input scaling.

### What's Staying the Same
- Stop/terminate semantics, the prompt flow, the media gate.

## Success Criteria
- [ ] A start against a desktop that refuses the password fails `Unauthenticated` with the server's reason, and no bridge process remains
- [ ] A successful start returns the bridge's reported size, not 1920×1080
- [ ] A bridge that exits or stays silent past the timeout fails the start and is reaped
- [ ] The host viewer shows "the desktop refused the sign-in" and no longer waits
- [ ] A click on a 1280×800 desktop lands where aimed

## Out of Scope
- Re-asking for credentials (node 8); mid-session resize (TODO).
