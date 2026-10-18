# Screen sharing on the Hosts screen — PRD

**Date**: 2026-10-18
**PRD Type**: Feature Enhancement
**Stack**: `#host-desktops` 4/8. Umbrella on `feature/host-desktops/service-split`.

## Affected Features

- [Hosts screen](../hosts-screen.md): rows gain the tooling section
- [Hosts screen tooling](../hosts-screen-tooling.md): § "Not yet reachable in the running app" is removed
- [Screen sharing sessions](../screen-sharing-sessions.md): AC-SS-10 becomes reachable

## Summary

Each online row on the Hosts screen asks its host for `GetHostTooling` and renders the tooling
section, including remote desktop with its Connect action. The daemon caches the probe result per
host for a short time and coalesces concurrent probes into one, so a Hosts screen listing many hosts
does not fan out subprocesses on every visit.

## Background

`HostRowTooling` → `HostRowRemoteDesktop` → `HostDesktopOverlay` are built and covered by Cypress
specs, but `HostsScreen.tsx` renders only `HostRowTelemetry`, and nothing on the screen calls
`GetHostTooling`. The probe has no cache and no in-flight dedup; the TODO says to fix that "before
it does, not after".

## Proposed Changes

### What's Changing
- `HostsScreen` renders, per online row, a tooling cell fed by `GetHostTooling` to that host (via `useHostClient(HostService, hostId)`), with loading and failure states.
- Offline rows show no tooling (nothing to ask).
- `host_tooling.rs`: a per-(host, hostname) result cache with a TTL, plus in-flight dedup (concurrent callers share one probe). Mirrors the `list_agent_models` cache shape.

### What's Staying the Same
- The probe's content and its no-bytes rule; the Connect gating (media, reachable, can bridge).

## Success Criteria
- [ ] In the running app, a reachable desktop on an online host shows Connect on its Hosts row
- [ ] Offline hosts make no tooling call
- [ ] Two concurrent `GetHostTooling` calls for one host run one probe
- [ ] A second call within the TTL runs no probe

## Out of Scope
- Several channels per row (node 7); a manual refresh control.
