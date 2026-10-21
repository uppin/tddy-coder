# 2026-10-18 — a remote desktop cannot change size mid-session

**Category:** Future enhancement
**Source:** `#host-desktops` 6/8 (`feature/host-desktops/bridge-ready`)

- `packages/tddy-screenshare/src/streamer.rs:134-141` `push_rgba_frame` refuses a frame whose size
  differs from the one the track was published at, so a desktop that changes resolution after the
  stream starts (an RDP client-side resize, `xrandr` on an X display, a VNC DesktopSize update) stops
  updating.
- `#host-desktops 6/8` made the start reply carry the real size the bridge saw, and the viewer scales
  input into it — but only the **initial** size. A resize would also need the new size sent to the
  browser (the input scaling reference), not only a republished track.
- **Why deferred:** the stack's scope was opening desktops and signing in; resize needs a
  track-republish path in the streamer plus a size-change message on the input stream, which no
  node in the stack builds. Nothing in the stack blocks it.
