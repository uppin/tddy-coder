# Changeset: Sessions drawer and project home in `tddy-session-web`

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages, ~60 integration specs across six features

**Stack**: `#web-split` 18/20 — branch `feature/web-split/session-drawer`, base `feature/web-split/session-inspector`

**PR**: [#862](https://github.com/uppin/tddy-coder/pull/862)

## Initial Discovery

[2026-11-07-session-drawer-package-initial-discovery.md](./2026-11-07-session-drawer-package-initial-discovery.md)

## Affected Packages

- **tddy-session-web**: receives this node's modules and specs.
- **tddy-web**: the moved modules leave; importers are rewritten.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-session-drawer-package.md](../../ft/web/1-WIP/PRD-2026-11-07-session-drawer-package.md)

## Summary

Move the sessions drawer and project home into `tddy-session-web`: `SessionsDrawerScreen`, `SessionDrawer*`, `ProjectHomeEntry`, `useSessionsDrawer*`, `useEnsureProjectHome`, `useProjectHomeSetup`, `NewTerminalMenu`, `AgentPicker`, `StatusBar`, `SessionTrafficStrip` — and the ~60 drawer-driven specs other nodes left behind (PR-stack, create-session, host, provider-defaults).

## Background

See the PRD (split of #851).

## Responsibility

- The drawer/project-home modules named in the Summary, and the ~60 drawer-driven specs with their support.

## Boundaries

- Not storybook or wiring (next nodes).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#851` session seams | `tddy-session-web` package, AppLocation port, shell slot, passphrase slot, session helpers, rpc pieces | imports them | re-create seams or move helpers |
| earlier package nodes | terminal, chat, hosts, workspace, prstack, create-session, LiveKit ports | composes them | move their code |
| `session-runtime`, `session-inspector` | runtime, terminals, inspector, agents, code pane | the drawer composes them | move their code |

## Draft PR contract

**As pushed (commit 2):** the 104 drawer-driven specs (every tddy-web component spec that mounts the drawer,
project home, the new-terminal menu, status bar or traffic strip — including the PR-stack, create-session, host,
activity/subagent and provider-defaults specs earlier nodes left behind) `git mv`'d into
`tddy-session-web/cypress/component/`, imports re-pointed (`components/sessions/` → `src/`, `components/session/` →
`src/code/`); the boundary test's surface gains `SessionsDrawerScreen`, `SessionDrawer`, `ProjectHomeEntry`,
`StatusBar`, `SessionTrafficStrip`, `NewTerminalMenu`. `AgentPicker` is **not** here: `SessionMainPane` imports it, so
it moves with #861 as one of that node's prerequisites. Red: the boundary test. LiveKit-importing specs switch to the
port doubles at green.

Originally planned:

First push (commit 2): the specs listed in `## Responsibility` `git mv`'d into `tddy-session-web` with
imports pointing at the package, and the package boundary test's surface list extended with this node's
exports — failing until the modules move.

## Green wave

**Wave:** 10 of 11
**Greenable independently:** after the inspector node is green
**Blocks:** storybook, wiring

## Progress (green, in flight)

- Moved into `tddy-session-web/src` (flat, `git mv`): the whole of `tddy-web/src/components/sessions/` (drawer, project home, `StatusBar`, `SessionTrafficStrip`, session manager, hooks, `inspectorState`, `jailedCodebaseHalf`, `useMeterSnapshot`, `SessionDetailPane`, `SessionHostStatsFooter`, `UploadProgressIndicator`, `vncInput`, ...). `components/sessions/` and `components/session/` in tddy-web now hold nothing. `bun test src` 376 pass (boundary rule green); tddy-web `test:unit` 93 pass.
- New seam: `SessionTrafficProvider` / `useSessionTraffic` (meter registry + `roomOf`) so `StatusBar` needs neither tddy-web nor LiveKit; `appClientsPorts.tsx` supplies it. The drawer reads auth through `tddy-client-web`'s `useAuth`.
- Specs: support copied/rewritten on ports (`drawerWorld`, `withSelectedDaemon`, `inMemory`, `recordingLiveKitRpc`, `perDaemonLiveKitRpc`, `hashNavigation`, `sessionShellDouble`). Kept in tddy-web (wiring): `PresenceCapabilityGatingAcceptance`, `UnifiedLayoutAcceptance`, `DaemonChangeReloadsScreenAcceptance`, `SelectedHostUrlStateAcceptance`, `ProjectHomeSetupLinkAcceptance`, `useLiveKitTerminalToken`.
- `dom-preload.ts` + `bunfig.toml` moved to the package (only `vncInput.test.ts` read it).
- **Cypress state: NOT green.** 101 moved/changed specs run one at a time: ~70 pass; ~31 specs still have failures (~100 of 646 tests): create-session, PR-stack chat/start, screen-sharing/VNC target rows, LiveKit routing specs (`SessionParticipantRpcRouting`, `SessionStartResumeStillDaemonRouted`, `ScreenSharingInputScopes`, `HostDesktopStartFailure` — candidates to return to tddy-web), and inspector-open specs (inspector defaults to closed since #378; unverified whether they fail on master too).

## Scope

- [ ] Modules moved
- [ ] Drawer-driven specs moved, on port doubles
- [ ] Boundary rule green

## Technical Changes

### State A (Current)

Drawer and project-home modules in `tddy-web/src/components/sessions/`; ~60 drawer-driven specs in tddy-web.

### State B (Target)

They live in `tddy-session-web`.

## Testing Plan

Cypress component specs from `tddy-session-web`'s config, one at a time locally; `bun test src`;
boundary rule. CI: the stack's single `web.yml` (affected-package shards).

## Acceptance Tests

- The ~60 moved specs, unchanged assertions; `src/packageBoundary.test.ts`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- Split from #851 (developer, 2026-11-07); see the PRD background.

## Refactoring Needed

## Validation Results

## TODO

- [x] Record initial discovery
- [x] Cross-check code issues and TODOs (inherited from #851's discovery)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Final validation (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
