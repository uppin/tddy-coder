# Hosts package (`tddy-hosts-web`)

**Package:** `packages/tddy-hosts-web`
**Applies to:** the Hosts and VMs screens, the host stats footer and indicators, and the
screen-sharing, VNC and host-desktop overlays sessions and hosts open
**Added:** 2026-11-07 (#web-split 12/17, PR #848)

## Overview

Hosts are one package: the Hosts screen and its rows, host key continuity, the passphrase and desktop
sign-in dialogs, the host stats feed and its indicators, the desktop overlays and the VM screen. It
reaches daemons through `tddy-client-web` (media through the media port) and renders with
`tddy-ui-web`; it does not depend on the web app or on LiveKit. The page shells stay in the web app.

## Behaviour

- Hosts, VMs, host stats, screen sharing and VNC behave as before ([hosts-screen.md](hosts-screen.md),
  [host-stats-footer.md](host-stats-footer.md), [screen-sharing-sessions.md](screen-sharing-sessions.md),
  [vnc-sessions.md](vnc-sessions.md)).
- Opening a host desktop that asks for a sign-in shows the sign-in form above the "Connecting…"
  screen; before, the connecting screen covered it.

## Acceptance criteria

- [x] Hosts live in `tddy-hosts-web` and import nothing from `tddy-web` or LiveKit (rule test).
- [x] Its specs pass from the package (`web.yml` runs them).
- [x] Sessions render the host indicators and overlays unchanged (`tddy-web`'s session and wiring specs pass).

Technical reference: [packages/tddy-hosts-web/docs/README.md](../../../packages/tddy-hosts-web/docs/README.md).
