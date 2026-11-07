# Hosts package (`tddy-hosts-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 11/16 — branch `feature/web-split/hosts`

## Affected Features

- **Primary**: [hosts-screen.md](../hosts-screen.md), [hosts-screen-add-key.md](../hosts-screen-add-key.md), [hosts-screen-telemetry.md](../hosts-screen-telemetry.md), [hosts-screen-tooling.md](../hosts-screen-tooling.md), [host-stats-footer.md](../host-stats-footer.md), [screen-sharing-sessions.md](../screen-sharing-sessions.md), [vnc-sessions.md](../vnc-sessions.md).
- **Related**: VMs screen, demo VM controls.

## Summary

Move the hosts experience — `HostsScreen` and its rows (telemetry, tooling, SSH agent/config, remote desktop), host key continuity, passphrase and desktop sign-in dialogs, the host stats footer and the CPU/disk/memory/load indicators with `hostStatsFormat`, the screen-sharing and VNC overlays, the VMs screen and `DefineVmPanel`, `DemoVmControls` — into `packages/tddy-hosts-web`, with its ~30 specs. The `*AppPage` wrappers stay as wiring.

## Background

Hosts reach into `sessions/` today only for the indicators and `ScreenSharingOverlay`; moving those here removes the cycle, and the screen-sharing/VNC media is behind `MediaPort` since `feature/web-split/livekit`.

## Proposed Changes

### What's Changing

- New package; indicators, overlays and VM UI move in; its PR check.

### What's Staying the Same

- Host listing, add-key, telemetry, tooling, remote desktop, screen sharing, VNC and VM behaviour.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] Hosts live in `tddy-hosts-web` with no `tddy-web`/LiveKit import.
- [ ] Its specs pass from its own PR check.
- [ ] Sessions render the host indicators and overlays unchanged.

## References

- Predecessors (real): `livekit` (media port), `clients`, `ui`. Successor: `session`.
