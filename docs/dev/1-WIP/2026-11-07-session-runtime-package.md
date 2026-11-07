# Changeset: Session runtime and terminals in `tddy-session-web`

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages, ~50 specs, runtime registry ordering, an inherited oversized-file record

**Stack**: `#web-split` 16/20 — branch `feature/web-split/session-runtime`, base `feature/web-split/session`

**PR**: [#860](https://github.com/uppin/tddy-coder/pull/860)

## Initial Discovery

[2026-11-07-session-runtime-package-initial-discovery.md](./2026-11-07-session-runtime-package-initial-discovery.md)

## Affected Packages

- **tddy-session-web**: receives this node's modules and specs.
- **tddy-web**: the moved modules leave; importers are rewritten.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-session-runtime-package.md](../../ft/web/1-WIP/PRD-2026-11-07-session-runtime-package.md)

## Summary

Move the session runtime and its terminal surfaces into `tddy-session-web`: `SessionRuntime`, `sessionRuntimeRegistry`, `sessionNotificationRegistry`, `GrpcSessionTerminal`, `SessionTerminalTabs`, `SessionChildRuntime`, `Dormant*`, `useSession*`, `useTerminal*`, `useTmuxSessions`, `attachClaim`.

## Background

See the PRD (split of #851).

## Responsibility

- The runtime/terminal modules named in the Summary, and the ~50 specs named in the PRD, with their support.

## Boundaries

- Not the inspector/agents/code pane (next node) or the drawer (the one after).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#851` session seams | `tddy-session-web` package, AppLocation port, shell slot, passphrase slot, session helpers, rpc pieces | imports them | re-create seams or move helpers |
| earlier package nodes | terminal, chat, hosts, workspace, prstack, create-session, LiveKit ports | composes them | move their code |

## Draft PR contract

**As pushed (commit 2, on #851's green 8d4904b):** 21 runtime specs `git mv`'d from tddy-web into
`tddy-session-web/cypress/component/` (classified by what they mount: runtime modules, no inspector or drawer);
their `src/components/sessions/` imports re-pointed at the package `src/` (`components/session/` → `src/code/`).
The boundary test's surface gains `SessionRuntime`, `SessionMainPane`, `SessionTerminalTabs`, `GrpcSessionTerminal`,
`SessionRuntimeRegistry`, `SessionChildRuntime`. Red: the boundary test (moved specs still reach tddy-web; surface
missing). LiveKit-importing specs among them switch to the port doubles at green.

**Re-planned at green (orchestrator decision, 2026-11-07):** `SessionMainPane` (+ `sessionMainPaneProps`) does
not move here: it composes the inspector, agent layers, code pane, picker and sidebar, which move with #861.
It leaves the boundary test's surface list, and the six specs that mount it (`GhosttyTerminalShrink`,
`PrStackPresenterRoomAcceptance`, `SessionInspectorMediaCapabilityAcceptance`, `SessionMainPaneRoomTerminal`,
`SessionMainPaneTerminalControl`, `SessionMainPaneUnscopedSession`) go back to `packages/tddy-web/cypress/component/`
byte-for-byte (import paths aside) for #861/#862. 15 specs stay. `useConversationHostAttachment` (runtime
attachment state, imported by `SessionRuntime`/`DormantSessionRuntime`) moves into the package; the layer stays in
tddy-web and imports it from there.

Originally planned:

First push (commit 2): the specs listed in `## Responsibility` `git mv`'d into `tddy-session-web` with
imports pointing at the package, and the package boundary test's surface list extended with this node's
exports — failing until the modules move.

## Green wave

**Wave:** 8 of 11
**Greenable independently:** after #851 is green (it consumes #851's seams)
**Blocks:** inspector and drawer nodes, storybook, wiring

## Scope

- [x] Modules moved
- [x] Specs moved, on port doubles
- [x] Boundary rule green

## Technical Changes

### State A (Current)

Runtime and terminal modules in `tddy-web/src/components/sessions/`.

### State B (Target)

They live in `tddy-session-web`; tddy-web imports them.

## Testing Plan

Cypress component specs from `tddy-session-web`'s config, one at a time locally; `bun test src`;
boundary rule. CI: the stack's single `web.yml` (affected-package shards).

## Acceptance Tests

- The ~50 moved specs, unchanged assertions; `src/packageBoundary.test.ts`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- Split from #851 (developer, 2026-11-07); see the PRD background.

## Deviations (green)

- Moved with the runtime, as helpers only it uses: `sessionRuntimeProps`, `useSessionTerminals`,
  `useSessionTerminalFeed`, `useSessionAttachment`, `useChildSessions`, `useAgentTerminalFocus`,
  `useSessionPaneFullscreen`, `agentConversationTabs`, `SessionPaneChrome`, `TerminalControlOverlay`,
  `SessionConnectionOverlay`, `NewTerminalMenu`, `tmuxSessionName`, `rpc/connections/hostServedSession`,
  `rpc/connections/useConnectionStatus`, `lib/terminalGridMeasure`, and the new
  `useConversationHostAttachment` (cut out of `SessionAgentConversationLayer`, which stays in tddy-web).
- `sessionRuntimeRegistryMetadataMerge.test.ts` stays in tddy-web: it builds a `SessionManager`, which
  moves with the drawer.
- `SessionConnectionAcceptance` and `useSessionAttachment` specs no longer build a
  `LiveKitConnectionProvider`; they run on a host-connection double whose `openSession` is the production
  `openHostServedSession`. The one assertion that was about the provider's token mint
  (`cy.wait("@generateToken")`) is dropped here; the mint is covered by `tddy-livekit-web`.
- `tddy-session-web` gains `tddy-terminal-web`, `tddy-session-create-web` (workspace) and `lucide-react`
  (already in the repo) as dependencies; `bun.lock`'s workspace entry edited by hand (the local-registry
  install restores the committed lock).
- Package Cypress support (`testIds` subset, `protoRpc`, `sessionConnections`, `hostConnections`,
  `terminalControlPage`) is copied; tddy-web keeps its own.

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
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [x] Run the touched packages' tests — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Final validation (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
