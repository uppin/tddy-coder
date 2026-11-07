# Changeset: Session inspector, agents and code pane in `tddy-session-web`

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages, ~40 specs, media/presence/room-RPC port consumers

**Stack**: `#web-split` 17/20 — branch `feature/web-split/session-inspector`, base `feature/web-split/session-runtime`

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

First push (commit 2): the specs listed in `## Responsibility` `git mv`'d into `tddy-session-web` with
imports pointing at the package, and the package boundary test's surface list extended with this node's
exports — failing until the modules move.

## Green wave

**Wave:** 9 of 11
**Greenable independently:** after the runtime node is green
**Blocks:** drawer node, storybook, wiring

## Scope

- [ ] Modules moved
- [ ] Specs moved, on port doubles
- [ ] Boundary rule green

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

## Validation Results

## TODO

- [x] Record initial discovery
- [x] Cross-check code issues and TODOs (inherited from #851's discovery)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
