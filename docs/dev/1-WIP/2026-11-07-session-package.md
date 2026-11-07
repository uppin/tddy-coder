# Changeset: Session package (`tddy-session-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 (re-scored after the split; was 5) — several row-4 signals: the largest move (~15k lines, ~90 specs), composes seven sibling packages, cuts the last upward reaches from `rpc`/`hooks`, an inherited oversized-file record; boundary may need a split (see Decisions)

**Stack**: `#web-split` 14/16 — branch `feature/web-split/session`, base `feature/web-split/prstack`

**PR**: [#851](https://github.com/uppin/tddy-coder/pull/851)

## Initial Discovery

[2026-11-07-session-package-initial-discovery.md](./2026-11-07-session-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ⚠ DURING — oversized `SessionMainPane` — `packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md`

Moves with the file; recorded, not fixed here.

### ⚠ DURING — dead session surfaces — [`2026-08-01-tddy-web-dead-session-surfaces.md`](../todo/2026-08-01-tddy-web-dead-session-surfaces.md)

The code moves; recorded, not fixed here.

## Affected Packages

- **tddy-session-web** (new): `packages/tddy-session-web/` — sources, Cypress specs, page objects/drivers/fixtures, README, `docs/`, moved code-issue records.
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the moved modules leave; consumers import from `tddy-session-web`; feature docs under `packages/tddy-web/docs/` for this area move to `packages/tddy-session-web/docs/` at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-session-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-session-package.md](../../ft/web/1-WIP/PRD-2026-11-07-session-package.md)

## Summary

Move the remaining session code — drawer, runtime, main pane and terminal tabs, inspector, agent layers, code pane — into its own package.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-session-web` exports `SessionsDrawerScreen`, `SessionDrawer`, `SessionRuntime`, `SessionMainPane`, `SessionTerminalTabs`, `GrpcSessionTerminal`, `SessionInspectorDrawer` and tabs, `SessionAgentRosterPane`/`SessionAgentTree`/`SessionAgentConversationLayer`, `ProjectHomeEntry`, `StatusBar`, `SessionTrafficStrip`, `WorktreeCodePane` and `components/session/*`, the session runtime registry and notification registry, and their hooks/helpers (`utils/session*`, `lib/session*`).
- Moved specs (156 — see `## Draft PR contract`; originally ~90: `Session*`, `Sessions*`, `Dormant*`, `Durable*`, `FastSession*`, `GrpcSession*`, `ConnectedSession*`, `UnifiedLayout*`, `UseTmux*`, `CodeHover*`, `CodeIndex*`, `RestructurePlan*`, `WorktreeCodePane*`) and their page objects/drivers.
- Its boundary rule test `packages/tddy-session-web/src/packageBoundary.test.ts` and `web-tddy-session-web.yml`.

## Boundaries

- Does not move shell, routes, `*AppPage`s, auth wiring or the app entry (`feature/web-split/wiring`).
- Does not split `SessionMainPane.tsx` (record moves with it).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, theme, shared helpers | renders with them | move UI primitives |
| `n5` clients (#842) | generated clients, `ClientsPort`/`NavigationPort`/`AuthPort` hooks, `mountWithClients` | all RPC/navigation/auth through them; specs mount with `mountWithClients` | add a transport or provider |
| `n6` livekit (#843) | presence/media ports | inspector media tabs, drawer participants consume them | import `livekit-client` |
| `n7` create-session (#844) | create-session pane/dialog | drawer and project home open them | move it |
| `n8` terminal (#845) | terminal components and feed | terminal tabs render them | move it |
| `n10` chat, `n11` hosts, `n12` workspace, `n13` prstack | their packages | the session layout composes them | move them |

## Draft PR contract

**Re-planned 2026-11-07 (commit 3) — split into four nodes.** This node's first [G] stopped on its boundary
warning before moving code: 150 files / 19,076 lines, 156 specs, ~40 reaches into tddy-web the plan never named
(`routing/useAppLocation`, `routing/appRoutes`, `shell/AppShell`, `HostStartPassphrasePrompt`, 14 `lib/`, 7 `utils/`,
8 `rpc/` modules, `hooks/authProvider`, `GhosttyTerminalSession`, `EnqueuedInputOverlay`, `test-utils`), and 10 specs
importing LiveKit testkits. The developer chose to split by capability:

- **This node (15a): session seams and helpers.**
  - Seams: session code reads the app location through `tddy-client-web`'s `NavigationPort` (`useAppLocation`) and
    `appRoutes` params through it; `AppShell` and `HostStartPassphrasePrompt` reach session code only through slots the
    app fills (create-session already has `StartPassphraseProvider` — reuse it).
  - Helpers into `tddy-session-web`: `lib/{sessionIndicator, sessionParticipantMetadata, shellQuote, toolShortcuts,
    sessionsRefreshBridge, clipboard}`, `utils/{crossHostSessions, projectHomeGroups, sessionProjectGroups,
    sessionDrawerLabel, sessionSort, sessionDisplay, sessionSelection, connectionStatusForSession, …}`,
    `rpc/{trafficMeter, uploadProgress, daemonTransportFlavour, livekitPing, useSessionNotifications}` — final list by
    consumers; a helper with a non-session consumer goes to the package that consumer can reach.
  - No session component moves here.
- `feature/web-split/session-runtime` (#860), `session-inspector` (#861), `session-drawer` (#862) move the components
  and their specs.
- LiveKit-importing session specs use the port doubles (developer's choice), in whichever node moves them.

Commit 3 moves the 156 specs commit 2 had moved **back** to tddy-web (byte-identical to the base), keeps the package
skeleton, and replaces the red tests: `src/sessionSeams.test.ts` (3 seam rules) and the boundary test's surface list
(the helpers' real export names). Red: 4 failing.

Earlier contract (commit 2):

**As pushed (commit 2):** skeleton, workflow, boundary rule; **156** specs `git mv`'d — every remaining
`tddy-web` component spec that mounts session code, **including** the specs earlier nodes left behind because
they drive their feature through the sessions drawer (5 create-session, 9 terminal, 7 subagent/activity,
6 host, 36 PR-stack, 1 workflow-chat routing, 1 provider-defaults). Imports of `src/components/sessions/` →
package `src/`, `src/components/session/` → `src/code/`. Left in `tddy-web` for `feature/web-split/wiring`:
`App`, `AppShell`, `DaemonSelector*` (3), `DesktopIpcHostAcceptance`, `ServerStreamFallback`,
`useConnectionStatus`, `worktreeToolParity`; LiveKit specs are `feature/web-split/livekit`'s.

Originally planned:

First push (commit 2): the `tddy-session-web` skeleton (manifest exporting the surface in `## Responsibility`, Cypress/Vite config, workflow), the specs, page objects and drivers listed below `git mv`'d into the package with imports pointing at it, and the boundary rule — failing until the modules move.

## Green wave

**Wave:** 7 of 9
**Greenable independently:** yes — specs mount session screens against in-memory RPC and port doubles; every composed package is published on the base
**Concurrent with:** none (only node in wave 7)
**Blocks:** `storybook`, `wiring`

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] `tddy-session-web` skeleton + workflow
- [ ] Modules moved; `tddy-web` (and sibling packages) import from `tddy-session-web`
- [ ] Specs, page objects, drivers moved and green from the package config
- [ ] Boundary rule
- [ ] Code-issue records and package docs moved

## Technical Changes

### State A (Current)

`components/sessions/` after the earlier nodes: ~15k lines (drawer, runtime, main pane, tabs, inspector, agent layers), `components/session/` (2.9k, code pane); `rpc`/`hooks` reach into sessions for `sessionNotificationRegistry`, `worktreeStatsFormat` (moved by #workspace) and attachments (moved by #844); `SessionMainPane` recorded oversized.

### State B (Target)

`tddy-session-web` owns sessions; `tddy-web` imports it from the app routes only.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [ ] `tddy-session-web` specs green
- [ ] `tddy-web` builds; consumers' scoped specs green

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-session-web

- Moved: the ~90 session specs listed in `## Responsibility` — unchanged assertions.
- Moved `bun test` suites under `components/sessions/` and `components/session/`.
- `src/packageBoundary.test.ts` — `tddy-session-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Complexity 5 — boundary warning.** If pass 2 shows the drawer and the runtime/inspector cannot move as one reviewable PR, split by capability (drawer first, runtime + inspector second) before the contract commit.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-session-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`tddy-session-web` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
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
