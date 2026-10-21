# 2026-10-20 — endpoint desktop channels still find their host target by `(127.0.0.1, port, protocol)`

**Category:** Future enhancement
**Source:** `#host-desktops` 7/8 (`feature/host-desktops/x11-channels`, PR #659)

- `packages/tddy-web/src/components/hosts/HostDesktopOverlay.tsx` `targetForChannel`: an X11
  channel finds its target by `(protocol, x11Display)` with no host or port. An **endpoint**
  channel (`vnc:5900`, `rdp:3389`) still matches `target.host === PROBED_HOST ("127.0.0.1") &&
  target.port === port && target.protocol === protocol`, and adds one with `host: PROBED_HOST`.
- #659's Responsibility line reads "`HostDesktopOverlay` opens the channel it is given (no
  `PROBED_HOST`/fixed-port matching)". That holds for X11 channels and for the port (taken from the
  reading, never a constant). The loopback host stays for endpoint channels.
- **Why kept:** the endpoint probe connects only to `127.0.0.1`
  (`remote_desktop_probe.rs` `PROBE_HOST`), so the loopback host *is* that channel's address.
  Dropping it from the match would let an operator-added target for another machine on the same
  port and protocol be opened in its place, and the added target must name the address the bridge
  connects to, which for an endpoint channel is the loopback the probe found it on. No spec pins
  the host half of the match today (`HostDesktopConnectAcceptance.cy.tsx` checks that the probed
  endpoint is added, not with which host); a spec seeding a non-loopback target on `:5900` would.
- **What would close it:** a host target that records the `channel_id` it was created for
  (`AddHostTargetRequest.channel_id`, carried on `ScreenSharingTarget`), so every channel is
  matched by identity and `PROBED_HOST` leaves the web client. Only worth it once endpoint probes
  stop being loopback-only, i.e. when port discovery (out of scope in
  `docs/ft/web/screen-sharing-sessions.md`) lands.
