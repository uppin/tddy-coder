# Changeset: Session inspector, agents and code pane in `tddy-session-web`

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages, ~40 specs, media/presence/room-RPC port consumers

**Stack**: `#web-split` 17/20 — branch `feature/web-split/session-inspector`, base `feature/web-split/session-runtime`

**PR**: [#861](https://github.com/uppin/tddy-coder/pull/861)

## Initial Discovery

[2026-11-07-session-inspector-package-initial-discovery.md](./2026-11-07-session-inspector-package-initial-discovery.md)

## Affected Packages

- **tddy-session-web**: receives this node's modules and specs.
- **tddy-web**: the moved modules leave; importers are rewritten.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-session-inspector-package.md](../../ft/web/1-WIP/PRD-2026-11-07-session-inspector-package.md)

## Summary

Move the session inspector and its tabs (details, activity, files, tools, usage, worktree, screen sharing, VNC), `InspectorTabs`, the agent roster/tree/conversation layers, `WorktreeCodePane` and `components/session/*` (to `src/code/`) into `tddy-session-web`.

## Background

See the PRD (split of #851).

## Responsibility

- The inspector, agent and code-pane modules named in the Summary, and their ~40 specs with support.

## Boundaries

- Not the runtime (previous node) or the drawer (next node).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#851` session seams | `tddy-session-web` package, AppLocation port, shell slot, passphrase slot, session helpers, rpc pieces | imports them | re-create seams or move helpers |
| earlier package nodes | terminal, chat, hosts, workspace, prstack, create-session, LiveKit ports | composes them | move their code |
| `session-runtime` | runtime + terminal modules in the package | the inspector reads runtime state | move runtime code |

## Draft PR contract

**As pushed (commit 2):** 19 inspector/agent/code-pane specs `git mv`'d into `tddy-session-web/cypress/component/`
(imports re-pointed), and the boundary test's surface gains `SessionMainPane`, `SessionInspectorDrawer`,
`SessionAgentRosterPane`, `SessionAgentTree`, `SessionWithActivitySidebar`, `WorktreeCodePane`,
`SessionIndexingIndicator`, `AgentPicker`.

**Scope widened (orchestrator, approved plan change):** `SessionMainPane` (+ `sessionMainPaneProps`, 504 lines, recorded
oversized — must not grow) moves **here**, not in #860, because it composes this node's pieces. Its prerequisites move
with it: `SessionInspectorDrawer`, `SessionWithActivitySidebar`, `SessionAgentConversationLayer` (minus
`useConversationHostAttachment`, which #860 moves), `WorktreeCodePane`, `SessionIndexingIndicator` and `AgentPicker`
(listed for the drawer in the PRD — it moves here because `SessionMainPane` imports it). The specs that mount
`SessionMainPane` are returned to tddy-web by #860's green; this node moves them into the package (they are red until
`SessionMainPane` moves). Red: the boundary test.

Originally planned:

First push (commit 2): the specs listed in `## Responsibility` `git mv`'d into `tddy-session-web` with
imports pointing at the package, and the package boundary test's surface list extended with this node's
exports — failing until the modules move.

## Green wave

**Wave:** 9 of 11
**Greenable independently:** after the runtime node is green
**Blocks:** drawer node, storybook, wiring

## Scope

- [x] Modules moved
- [x] Specs moved, on port doubles
- [x] Boundary rule green

## Technical Changes

### State A (Current)

Inspector, agent and code-pane modules in `tddy-web/src/components/{sessions,session}/`.

### State B (Target)

They live in `tddy-session-web`.

## Testing Plan

Cypress component specs from `tddy-session-web`'s config, one at a time locally; `bun test src`;
boundary rule. CI: the stack's single `web.yml` (affected-package shards).

## Acceptance Tests

- The ~40 moved specs, unchanged assertions; `src/packageBoundary.test.ts`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- Split from #851 (developer, 2026-11-07); see the PRD background.

## Refactoring Needed

## Deviations (green)

- **Moved**: 60 modules (`sessions/*` flat into `src/`, `session/*` into `src/code/`, plus `hooks/useKeyedActionFailure` and
  `lib/codeLanguage`, which only they used) with their unit tests. `inspectorState`, `jailedCodebaseHalf`, `useMeterSnapshot`
  and `SessionDetailPane` stay in tddy-web: the drawer hooks, status bar and traffic strip read them.
- **Dependencies**: `useSessionUsage`/`useCodeSession` import `AsyncQueue` from `tddy-rpc-web` (where it lives; the
  package may not import `tddy-livekit-rpc-web`). The package gains `tddy-chat-web`, `tddy-hosts-web`, `tddy-prstack-web`,
  `tddy-rpc-web`, `tddy-workspace-web`, `react-resizable-panels`, `react-syntax-highlighter` (+ its types), the last
  three moved out of tddy-web's `package.json`. `bun.lock` regenerated.
- `SessionAgentClient` was declared twice (runtime props and main-pane props); main-pane props now import it.
- **Specs**: the 19 inspector/agent/code-pane specs plus `GhosttyTerminalShrink`, `SessionInspectorMediaCapabilityAcceptance`,
  `SessionMainPaneRoomTerminal`, `SessionMainPaneTerminalControl`, `SessionMainPaneUnscopedSession` run in the package on
  `cy.mountWithClients`. Support copied (not moved: tddy-web specs still read it) into `cypress/support/`; `testIds.ts` gains
  the keys they use; `sessionMainPanePage` is the slice of `sessionsDrawerPage` they need.
- **`PrStackPresenterRoomAcceptance` stays in tddy-web** (returned from the package): both assertions are about the app's
  HTTP transport minting the presenter-room token (`GenerateToken` carries the signed-in session token through the auth
  gate), which the package cannot mount without tddy-web or livekit. It imports `SessionMainPane` from `tddy-session-web`.
- `SessionAgentRosterSplitSession`: the per-daemon LiveKit mount became `perHostClients` (peers answered per host through
  the connection registry). Its "targets" assertion now reads the hosts dialled as peers (deduplicated); the
  `daemon-<hostId>` identity mapping is pinned by `tddy-livekit-web/src/connections/liveKit.test.ts`.
- `SessionInspectorMediaCapabilityAcceptance`: `appLocationPage` hash set-up became the navigation port double;
  `LIVEKIT_SOURCE_ID` comes from `tddy-client-web`.
- Code issue `oversized-file-session-main-pane` moved to `packages/tddy-session-web/docs/code-issues/` (504 lines, unchanged).

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
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Run the touched packages' tests — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Final validation (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
