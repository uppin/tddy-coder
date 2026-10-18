# Host desktops: screen sharing from the Hosts screen — umbrella PRD

**Date**: 2026-10-18
**PRD Type**: Feature Enhancement
**Stack**: `#host-desktops`, 8 nodes. This umbrella lives on the root node (`feature/host-desktops/service-split`); each node carries its own PRD for its slice.

## Affected Features

- **Primary**: [Screen sharing sessions](../screen-sharing-sessions.md), the host scope (AC-SS-10…16) and § Current limits / Out of scope
- [Hosts screen](../hosts-screen.md) and [Hosts screen tooling](../hosts-screen-tooling.md), the remote-desktop section of a host row
- [Remote desktop input](../../../../packages/tddy-web/docs/remote-desktop-input.md), the viewer's keyboard capture

## Summary

An operator opens the Hosts screen and sees, on each host that exposes one, **every desktop that host
serves**: the classic VNC (`:5900`) and RDP (`:3389`) endpoints, plus **one channel per X11 display**
the daemon's user runs. Connecting to a channel asks for exactly the credentials that desktop
advertises: nothing, a password, username and password, or username, domain and password. A
**Remember** box stores them in the operator's credential vault. A wrong password is reported as a
wrong password, not as an endless "Connecting…". The desktop is a LiveKit video track rendered in a
viewer that can go **browser full screen**. Where the browser allows it, the keyboard is locked, so
Esc and system shortcuts reach the desktop.

## Background

Host-scoped desktops are built end to end in code (#460, #hosts-screen 8/8 and 9/9), with four gaps:

1. **Nobody can reach it.** `HostsScreen` never mounts `HostRowTooling`, so the Connect action exists only in Cypress specs.
2. **One prompted password is the only credential**, and the bridges can authenticate only VNC None/VncAuth and RDP TLS. VeNCrypt, Apple Remote Desktop and NLA servers (the Windows default) refuse.
3. **Only one fixed port per protocol is offered**, so a host with several X displays shows one desktop or none.
4. **A wrong password, or any desktop that is not 1920×1080, fails silently or mis-aims clicks.**

## Proposed Changes — by node

| K | Node | Delivers |
|---|---|---|
| 1 | `service-split` | `screen_sharing_service.rs` split along its recorded seams. No behaviour change |
| 2 | `rdp-nla` | RDP desktops requiring NLA (CredSSP) sign in, with a domain. The bridge credential shape becomes `Credentials { username, domain, password }` |
| 3 | `vnc-security` | VNC desktops offering VeNCrypt (Plain / TLSPlain) or Apple Remote Desktop sign in. An unknown advertised type no longer aborts the handshake |
| 4 | `hosts-mount` | The Hosts screen shows each row's remote-desktop section and its Connect action. The tooling probe is cached and de-duplicated |
| 5 | `viewer-fullscreen` | The desktop viewer enters and leaves browser full screen; Keyboard Lock where supported |
| 6 | `bridge-ready` | Starting a desktop waits for the bridge to sign in. A refused sign-in is reported distinctly, and the viewer uses the desktop's real size |
| 7 | `x11-channels` | One channel per X11 display of the daemon's user, served through a daemon-spawned loopback `x11vnc`. Several Connect actions per row |
| 8 | `sign-in-form` | A sign-in form shaped by the auth methods the desktop advertises (on-demand handshake), with Remember, re-asking after a refused sign-in |

## What's Staying the Same

- Rendering is always a daemon-produced LiveKit video track. There is no browser-side VNC/RDP client.
- The periodic tooling probe writes no bytes to a desktop. Auth methods are read only when the operator opens one.
- An unremembered credential is asked for on the host's encrypted prompt channel, used once and kept nowhere.
- The media gate: no Connect action on a connection that carries no media.
- Session-scoped targets and their inspector tab.

## Impact Analysis

- **Technical:** `screen_sharing.proto` and `host.proto` gain fields and one RPC (`InspectHostDesktop`). Bridges gain a stdout readiness protocol and an inspect mode. Daemon config gains `screen_sharing.x11vnc_binary_path` and an X11 socket directory. New Rust crates are likely for ARD (bignum DH, AES, MD5) and must be confirmed before they are added.
- **User:** remote desktops become usable from the Hosts screen for the first time, on far more servers.
- **Runtime dependency:** `x11vnc` on hosts that want X11 channels. Absent, those channels report "cannot bridge".

## Success Criteria (stack-wide)

- [ ] An operator opens a host's desktop from the Hosts screen in the running app.
- [ ] A host with displays `:0` and `:1` offers two X11 channels, and each opens its own display.
- [ ] VncAuth, VeNCrypt Plain, ARD, RDP TLS and RDP NLA desktops each sign in with the credentials the form asked for.
- [ ] A wrong password is reported as such, and the form asks again.
- [ ] Remembered credentials open the desktop without a prompt; unchecked, nothing is stored.
- [ ] The viewer goes full screen and back, and clicks land where aimed at any desktop size.

## Out of Scope

- Several desktops open at once (one viewer at a time)
- Desktop resize mid-session (the streamer refuses a frame size change; filed as a TODO)
- Scroll wheel, clipboard, audio, multi-monitor within one display
- Other users' X displays (would need supervisor brokering)
- Kerberos for RDP
