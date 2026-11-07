# Changeset: Hosts package (`tddy-hosts-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages (new package; sessions consumers), ~30 specs moved, a cycle cut (indicators, screen-sharing overlay), media port consumer

**Stack**: `#web-split` 11/16 — branch `feature/web-split/hosts`, base `feature/web-split/chat`

## Initial Discovery

[2026-11-07-hosts-package-initial-discovery.md](./2026-11-07-hosts-package-initial-discovery.md)

## Affected Packages

- **tddy-hosts-web** (new): `packages/tddy-hosts-web/` — sources, Cypress specs, page objects/drivers/fixtures, README, `docs/`, moved code-issue records.
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the moved modules leave; consumers import from `tddy-hosts-web`; feature docs under `packages/tddy-web/docs/` for this area move to `packages/tddy-hosts-web/docs/` at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-hosts-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-hosts-package.md](../../ft/web/1-WIP/PRD-2026-11-07-hosts-package.md)

## Summary

Move the hosts and VMs screens, host stats indicators and the screen-sharing/VNC overlays into their own package.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-hosts-web` exports `HostsScreen`, the host row components, `HostKeyContinuityNotices`, `HostPassphraseDialog`, `HostDesktopSignInDialog`, `HostDesktopOverlay`, `HostStatsFooter`, `CpuCoresIndicator`, `DiskSpaceIndicator`, `MemoryIndicator`, `LoadAverageIndicator`, `hostStatsFormat`, `ScreenSharingOverlay`, `VncOverlay`, `VmsScreen`, `DefineVmPanel`, `DemoVmControls`, `useHostTooling`, `useHostPromptAnswer`, and the host-key helpers (`lib/hostKey*`).
- Moved specs (~30: `Host*`, `Vms*`, `VncOverlayAcceptance`, `ScreenSharing*`, `DefineVmPanel`, `WaitingHostSendsNoRpc`, `DesktopIpcHostAcceptance`) and `hostsScreenPage`/`hostStatsFooterPage`/`hostConnection*Page` page objects.
- Its boundary rule test `packages/tddy-hosts-web/src/packageBoundary.test.ts` and `web-tddy-hosts-web.yml`.

## Boundaries

- `HostsAppPage`/`VmsAppPage` stay in `tddy-web` (wiring).
- Session inspector tabs that *show* screen sharing/VNC (`SessionScreenSharingTab`, `SessionVncTab`) stay with sessions and render this package's overlays.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, theme, shared helpers | renders with them | move UI primitives |
| `n5` clients (#842) | generated clients, `ClientsPort`/`NavigationPort`/`AuthPort` hooks, `mountWithClients` | all RPC/navigation/auth through them; specs mount with `mountWithClients` | add a transport or provider |
| `n6` livekit (#843) | `MediaPort` (screen-sharing tracks, input forwarding, VNC media), `PresencePort` | overlays and host rows consume them | import `livekit-client` |
| `n7`–`n10` | — (branch predecessors only) | nothing | — |

## Draft PR contract

First push (commit 2): the `tddy-hosts-web` skeleton (manifest exporting the surface in `## Responsibility`, Cypress/Vite config, workflow), the specs, page objects and drivers listed below `git mv`'d into the package with imports pointing at it, and the boundary rule — failing until the modules move.

## Green wave

**Wave:** 5 of 9
**Greenable independently:** yes — specs mount hosts against in-memory host RPC and media/presence doubles
**Concurrent with:** `chat`
**Blocks:** `session` (indicators, overlays)

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] `tddy-hosts-web` skeleton + workflow
- [ ] Modules moved; `tddy-web` (and sibling packages) import from `tddy-hosts-web`
- [ ] Specs, page objects, drivers moved and green from the package config
- [ ] Boundary rule
- [ ] Code-issue records and package docs moved

## Technical Changes

### State A (Current)

`components/hosts/` (2.7k), `components/vms/` (0.4k), `DemoVmControls.tsx`; hosts imports `sessions/{CpuCoresIndicator,DiskSpaceIndicator,MemoryIndicator,ScreenSharingOverlay,hostStatsFormat}`; `ScreenSharingOverlay` and `VncOverlay` imported `livekit-client` before #843.

### State B (Target)

`tddy-hosts-web` owns hosts, VMs, indicators and overlays; sessions import them; the hosts → sessions edge is gone.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [ ] `tddy-hosts-web` specs green
- [ ] `tddy-web` builds; consumers' scoped specs green

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-hosts-web

- Moved: `Host*.cy.tsx`, `HostsScreen*.cy.tsx`, `VmsScreen.cy.tsx`, `DefineVmPanel.cy.tsx`, `VncOverlayAcceptance.cy.tsx`, `ScreenSharing*.cy.tsx`, `WaitingHostSendsNoRpc.cy.tsx`, `DesktopIpcHostAcceptance.cy.tsx` — unchanged assertions.
- Moved `bun test`: `hostRowFormat.test.ts`, `hostTelemetryState.test.ts`, `lib/hostKey*.test.ts`.
- `src/packageBoundary.test.ts` — `tddy-hosts-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Indicators and overlays move here**, not to sessions: they render host state; sessions show them.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-hosts-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`tddy-hosts-web` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
