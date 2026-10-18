# One desktop channel per X11 display — PRD

**Date**: 2026-10-18
**PRD Type**: Feature Enhancement
**Stack**: `#host-desktops` 7/8. Umbrella on `feature/host-desktops/service-split`.

## Affected Features

- [Hosts screen tooling](../hosts-screen-tooling.md): "lists both protocols, always — VNC on :5900 and RDP on :3389"
- [Screen sharing sessions](../screen-sharing-sessions.md): § Current limits "Only the default ports are probed" and § Out of scope "Discovering non-default desktop ports"

## Summary

A host serves **channels**. Besides the default VNC (`:5900`) and RDP (`:3389`) readings, the daemon
lists every X11 display its own OS user can open: the sockets in the X11 socket directory
(`/tmp/.X11-unix/X<N>`) it can connect to. Each display becomes an **X11 channel**: "Display :N".
Connecting to one makes the daemon start `x11vnc` for that display, bound to loopback on a free port
with no password. The bridge is pointed at it, and that `x11vnc` is stopped when the desktop closes
or its bridge ends. A display already served on `5900+N` by a VNC server is not listed twice. A host
row offers one Connect per connectable channel.

## Background

Today the probe reads one fixed port per protocol on `127.0.0.1`, `connectableDesktop` returns the
first match, and `HostDesktopOverlay` hard-codes `127.0.0.1:<port>`. A host with displays `:0` and
`:1` offers at most one desktop.

## Proposed Changes

### What's Changing
- `host.proto`: `HostRemoteDesktop` gains a channel identity (`channel_id`, `kind` = `ENDPOINT` | `X11_DISPLAY`, `display`, `label`). `GetHostToolingResponse.remote_desktop` lists every channel.
- Daemon config: `screen_sharing.x11vnc_binary_path` (resolved like the bridges) and `screen_sharing.x11_socket_dir` (default `/tmp/.X11-unix`). Without `x11vnc`, X11 channels report `can_bridge = false`.
- `AddHostTarget` accepts an X11 display target. `StartHostStream` for one spawns `x11vnc -display :N -localhost -rfbport <free> -nopw -forever`, waits until it accepts connections, then spawns the bridge at it. `StopHostStream` and bridge exit stop it.
- Web: one channel line and Connect per channel; the overlay opens the channel it was given (no fixed host or port).

### What's Staying the Same
- The probe writes no bytes to a desktop; X11 enumeration is a local socket listing plus a connect check, with no X protocol traffic.

## Success Criteria
- [ ] A host with displays `:0` and `:1` lists two X11 channels plus the endpoint readings
- [ ] A display the daemon's user cannot connect to is not listed
- [ ] Connecting to "Display :1" starts `x11vnc` for `:1` on loopback only and streams it
- [ ] Closing the desktop stops that `x11vnc`; reopening does not leave two
- [ ] Without `x11vnc` the channels are listed but offer no Connect
- [ ] A row with two connectable channels offers two Connect actions, each opening its own

## Out of Scope
- Other users' displays; Wayland; starting a new X server; non-Linux enumeration.
