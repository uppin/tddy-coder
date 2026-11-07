# Changeset: Agent chat package (`tddy-chat-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages (new package; sessions/prstack/models consumers), ~21 specs moved, a cycle cut (`agentActivityRegistry`)

**Stack**: `#web-split` 10/16 — branch `feature/web-split/chat`, base `feature/web-split/playground`

**PR**: [#847](https://github.com/uppin/tddy-coder/pull/847)

## Initial Discovery

[2026-11-07-chat-package-initial-discovery.md](./2026-11-07-chat-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ⚠ DURING — activities tail-first autoscroll follow-ups — [`2026-08-02-tddy-web-activities-tail-first-autoscroll-follow-ups.md`](../todo/2026-08-02-tddy-web-activities-tail-first-autoscroll-follow-ups.md)

The code moves; recorded, not fixed here.

## Affected Packages

- **tddy-chat-web** (new): `packages/tddy-chat-web/` — sources, Cypress specs, page objects/drivers/fixtures, README, `docs/`, moved code-issue records.
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the moved modules leave; consumers import from `tddy-chat-web`; feature docs under `packages/tddy-web/docs/` for this area move to `packages/tddy-chat-web/docs/` at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-chat-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-chat-package.md](../../ft/web/1-WIP/PRD-2026-11-07-chat-package.md)

## Summary

Move agent chat, ACP transcript/replay/tool-call hooks, the activity registry and activity/subagent panes into their own package.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-chat-web` exports `AgentChat`, `LiveAgentChatView`, `AgentTranscriptView`, `useAgentChat`, `useAcpSession`, `useAcpReplay`, `useAcpToolCallDetail`, `useTailFollow`, `chatTranscript`, `toolCallPresentation`, `chatEntryPresentation`, `acpAgentMerge`, `agentActivityRegistry` (from `sessions/`), the activity panes (`SessionActivitiesPane`, `AgentActivityOverlay`, `AgentActivityDetailDialog`) and the subagent conversation/permission UI (`SubagentConversationDialog`, `PermissionRequestEntry`, `TranscriptPermissionRequest`).
- Moved specs (~21: `AgentChat*`, `AgentActivity*`, `ActivitiesTailScrollAcceptance`, `Subagent*`, `InactiveSessionActivitiesAcceptance`) and `agentChatPage`/`agentActivityPage` page objects.
- Its boundary rule test `packages/tddy-chat-web/src/packageBoundary.test.ts` and `web-tddy-chat-web.yml`.

## Boundaries

- Does not move the session layers that host chat (`SessionAgentConversationLayer`, roster) — `feature/web-split/session`.
- Does not move `EnqueuedInputOverlay` (terminal, #845).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, theme, shared helpers | renders with them | move UI primitives |
| `n5` clients (#842) | generated clients, `ClientsPort`/`NavigationPort`/`AuthPort` hooks, `mountWithClients` | all RPC/navigation/auth through them; specs mount with `mountWithClients` | add a transport or provider |
| `n6` livekit (#843) | chat's room use behind `ClientsPort`/`MediaPort`; `livekit-client` gone from `useAgentChat`, `useAcpSession`, `AgentChat` | consumes those ports only | reintroduce a LiveKit import |
| `n7`–`n9` | — (branch predecessors only) | nothing | — |

## Draft PR contract

**As pushed (commit 2):** skeleton, workflow, boundary rule; **13** specs whose only session imports are
modules that move with chat, plus the `acpReplayProjection` and `toolCallPresentation` suites, `git mv`'d —
failing until the modules move. Chat-named specs that mount other session components stay in `tddy-web` for
`feature/web-split/session`.

Originally planned:

First push (commit 2): the `tddy-chat-web` skeleton (manifest exporting the surface in `## Responsibility`, Cypress/Vite config, workflow), the specs, page objects and drivers listed below `git mv`'d into the package with imports pointing at it, and the boundary rule — failing until the modules move.

## Green wave

**Wave:** 5 of 9
**Greenable independently:** yes — the specs mount chat against in-memory session RPC and port doubles; needs n6's port rewiring present on the base
**Concurrent with:** `hosts`
**Blocks:** `workspace` (models renders chat), `prstack`, `session`

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] `tddy-chat-web` skeleton + workflow
- [ ] Modules moved; `tddy-web` (and sibling packages) import from `tddy-chat-web`
- [ ] Specs, page objects, drivers moved and green from the package config
- [ ] Boundary rule
- [ ] Code-issue records and package docs moved

## Technical Changes

### State A (Current)

`components/chat/` (2.6k lines: `AgentChat`, `LiveAgentChatView`, `AgentTranscriptView`, ACP hooks, presentation helpers) imports `sessions/agentActivityRegistry` twice; sessions imports chat 20 times; models imports chat twice; prstack once. Activity panes and subagent dialogs live in `components/sessions/`.

### State B (Target)

`tddy-chat-web` owns chat and the activity registry; sessions/prstack/models import it; the chat → sessions edge is gone.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [ ] `tddy-chat-web` specs green
- [ ] `tddy-web` builds; consumers' scoped specs green

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-chat-web

- Moved: `AgentChat*.cy.tsx`, `AgentActivity*.cy.tsx`, `ActivitiesTailScrollAcceptance.cy.tsx`, `Subagent*.cy.tsx`, `InactiveSessionActivitiesAcceptance.cy.tsx` — unchanged assertions.
- Moved `bun test`: `acpReplayProjection.test.ts`, `toolCallPresentation.test.ts`.
- `src/packageBoundary.test.ts` — `tddy-chat-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **The registry moves with chat** — it is chat state that sessions observe, which breaks the cycle in the right direction.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-chat-package-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`tddy-chat-web` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
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
