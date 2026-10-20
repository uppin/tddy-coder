# 2026-10-20 — an X11 display `:N` (N ≥ 1) already served on `5900+N` is offered by no channel

**Category:** Behaviour gap
**Source:** `#host-desktops` 7/8 (`feature/host-desktops/x11-channels`, PR #659), final `/validate-changes`

- `packages/tddy-host-service/src/x11_displays.rs` `x11_channel_reading` drops a display whose VNC
  port (`5900 + N`) already answers, so it is not offered twice. That is right for `:0`, whose
  `:5900` server is the VNC endpoint reading. The endpoint probes read only `:5900` and `:3389`
  (`host_tooling.rs` `PROBED_PROTOCOLS` × `default_port()`), so for `:1` served by a VNC server on
  `:5901` (a hand-started `x11vnc` or `vncserver :1`) neither reading lists it, and the host row
  offers no Connect for a desktop that is plainly there.
- The PRD's criterion "a display already served on `5900+N` by a VNC server is not listed twice"
  holds. Being listed zero times is the gap. `does_not_list_a_display_already_served_on_its_vnc_port_twice`
  covers only `:0`/`:5900`.
- **Why deferred:** found in `/pr-wrap`'s final validation, after this stage's one return to
  `/green` was spent on the x11vnc reopen race. The fix changes the probe's output, so it needs its
  own test-first pass.
- **Fix shape:** in `x11_channel_reading`, when `5900 + N` answers and no endpoint reading already
  has that port, return the probe's own `Endpoint` reading for `5900 + N` (it already carries
  `can_bridge` and `desktop_reachable`), so the desktop is offered once, as the endpoint it is.
  Test: displays `:0` and `:1` with VNC served on `:5900` and `:5901`. Expect the VNC `:5900`
  endpoint, a VNC `:5901` endpoint, and no X11 channel.
