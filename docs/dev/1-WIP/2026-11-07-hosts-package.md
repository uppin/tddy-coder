# Changeset: Hosts package (`tddy-hosts-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages (new package; sessions consumers), ~30 specs moved, a cycle cut (indicators, screen-sharing overlay), media port consumer

**Stack**: `#web-split` 12/17 — branch `feature/web-split/hosts`, base `feature/web-split/media`

**PR**: [#848](https://github.com/uppin/tddy-coder/pull/848)

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

**As pushed (commit 2):** skeleton, workflow, boundary rule; **24** specs whose only session imports move
with hosts, plus the `hostRowFormat` and `hostTelemetryState` suites, `git mv`'d — failing until the modules
move. Host-named specs mounting other session components stay for `feature/web-split/session`.

Originally planned:

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

- [x] `tddy-hosts-web` skeleton + workflow (`web-tddy-hosts-web.yml` matches `web-tddy-react-commons.yml`, `has-cypress: true`)
- [x] Modules moved; `tddy-web` imports from `tddy-hosts-web`
- [x] Specs, page objects, drivers moved and green from the package config (17 specs; see Deviations)
- [x] Boundary rule
- [x] Code-issue records and package docs moved (no code-issue record names a moved file; package docs stay the README until wrap)

## Technical Changes

### State A (Current)

`components/hosts/` (2.7k), `components/vms/` (0.4k), `DemoVmControls.tsx`; hosts imports `sessions/{CpuCoresIndicator,DiskSpaceIndicator,MemoryIndicator,ScreenSharingOverlay,hostStatsFormat}`; `ScreenSharingOverlay` and `VncOverlay` imported `livekit-client` before #843.

### State B (Target)

`tddy-hosts-web` owns hosts, VMs, indicators and overlays; sessions import them; the hosts → sessions edge is gone.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [x] `tddy-hosts-web` specs green
- [x] `tddy-web` builds; consumers' scoped specs green

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-hosts-web

- Moved: `Host*.cy.tsx`, `HostsScreen*.cy.tsx`, `VmsScreen.cy.tsx`, `DefineVmPanel.cy.tsx`, `VncOverlayAcceptance.cy.tsx`, `ScreenSharing*.cy.tsx`, `WaitingHostSendsNoRpc.cy.tsx`, `DesktopIpcHostAcceptance.cy.tsx` — unchanged assertions.
- Moved `bun test`: `hostRowFormat.test.ts`, `hostTelemetryState.test.ts`, `lib/hostKey*.test.ts`.
- `src/packageBoundary.test.ts` — `tddy-hosts-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Green Phase Notes (2026-11-07)

Rebased onto `feature/web-split/media` (9654f9d7a); `MediaPort` consumed as published.

### Deviations from the draft contract

- **Specs that stay in `tddy-web`** (they exercise app wiring, not package code): `HostConnectionAcceptance`, `HostConnectionGateAcceptance`, `HostDirectoryAcceptance`, `HostLabelIdentityAcceptance`, `HostSelectionSurvivesReload` (selected-daemon provider, `DaemonSelector`, `ProjectsAppPage`), and `ScreenSharingInputForwardingAcceptance` / `ScreenSharingFullscreenAcceptance`, whose assertions are about the input stream the LiveKit-backed `MediaPort` opens to the bridge — a package that never imports LiveKit can only see a double. They now import the overlay from `tddy-hosts-web`. The package holds 17 specs (the 24 minus these 7).
- **`HostsAppPage` split**: the data container (`ListKnownHosts`, error line) is `HostsContent` in the package; `HostsAppPage` stays in `tddy-web` as `AppShell` + `HostsContent`. The one `HostsScreenAcceptance` case that opens the shell menu moved, unchanged, to `tddy-web/cypress/component/HostsAppPageNavigation.cy.tsx`.
- **`HostStatsFooter` takes `leading`/`trailing` slots** instead of `attachment`/`runtimes`/`runtimeRegistry`: the traffic `StatusBar` and `UploadProgressIndicator` are session code. `tddy-web`'s `SessionHostStatsFooter` composes them; `SessionsDrawerScreen` and `TerminalFileUploadProgressFooter` use it.
- **`HostDesktopOverlay` joins the bridge's room through `useMediaPort().usePresenterRoom`** (hint `{ sessionId: hostId, room, url }`) instead of `useCommonRoomHandle` + `presenceIdentityForUser` from `tddy-livekit-web`; the participant identity is now the presenter-room's own.
- **`HostStartPassphrasePrompt` stays in `tddy-web`** (sessions' create path mounts it and it depends on `tddy-session-create-web`); it consumes `HostPassphraseDialog`/`useHostPromptAnswer` from the package.
- **Shared helpers moved to `tddy-ui-web`** rather than duplicated: `browserFullscreen` (+test), `tddyMarker`, `formatTraffic` (+test). Moved into the package with their only consumers: `useHostStats`, `useHostPrompts`, both subscriptions (+tests), `encryptForHost`, `subtleCrypto`, `keyboardLock`, `screenSharingStartFailure`, `hostKey*`. `useAuthContext` became `useAuth` from `tddy-client-web`.
- **Spec support**: `cypress/support/mountWithHosts.tsx` (`cy.mountWithClients` + presence/media doubles; every listed host is reached over a LiveKit-shaped connection, as the shared common room made it) replaces `mountWithRpc` + `withSelectedDaemon`. The "calls carry the operator's session" case reads the token from `anAuthDouble`. Page objects and fixtures shared with remaining `tddy-web` specs are copied (`hostsScreenPage`, `screenSharingOverlayPage`, `hostPromptFeed`, `hostServiceBackend`, `hostConnections`, `hostKeys`); `testIds` carries only the ids the package uses. No stylesheet is loaded in the package's component support (as in `tddy-web`): a theme makes the connecting overlay and the sign-in dialog real layers at the same `z-50`, the former covering the latter.

### Verification (scoped)

- `tddy-hosts-web`: `bun test src` 63 pass; 17 Cypress specs one at a time, all green.
- `tddy-ui-web`: `bun test src` 21 pass. `tddy-web`: `bun run build` ok, `bun run test:unit` 867 pass.
- `scripts/livekit-import-boundary.test.ts`, `scripts/web-workflows.test.ts`: 4 pass.
- `tddy-web` Cypress (10 specs, one at a time): `HostsAppPageNavigation` 1, `HostDirectoryAcceptance` 8, `ScreenSharingInputForwarding` 15, `ScreenSharingFullscreen` 6, `HostDesktopStartFailure` 3 (failed once on the first run after dependency re-optimisation with a duplicate-React `useContext` error; green twice on re-run), `ScreenSharingInputScopes` 2, `SessionInspectorScreenSharing` 9, `HostResources` 3, `TerminalFileUploadProgressFooter` 2, `HostStatsFooterAcceptance` 6.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`tddy-hosts-web` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
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
