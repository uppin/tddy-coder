# Changeset: Session drawer grouped per project

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 2 — one package (tddy-web), a pure grouping util + one component; unit + Cypress component tests
**Stack**: #project-home 2/7. Base: `feature/project-home/home-agent-setting`.
**PR**: [#697](https://github.com/uppin/tddy-coder/pull/697)

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-22-project-home-project-groups-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Responsibility

Per-project grouping of the expanded session drawer: the `groupSessionsByProject` util, the
`SessionProjectGroup` component (header, count, collapse, persisted collapsed set), the trailing
**Other** group, and auto-expanding the group of the selected session.

## Boundaries

- No pinned entry inside a group — the project-home entry is node 4's.
- No change to the collapsed strip, bulk delete, cross-host badges or deep links.
- No proto or daemon change.
- Does not consume `home_agent` from node 1.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/project-home/home-agent-setting` (1/7) | `ProjectEntry.home_agent`, `SetProjectHomeAgent` | nothing — sits above it only because the stack is a line | read or render `home_agent` |

## Draft PR contract

```ts
// packages/tddy-web/src/utils/sessionProjectGroups.ts (new)
export interface ProjectSessionGroup { projectId: string | null; label: string; sessions: SessionEntry[] }
export function groupSessionsByProject(sessions: SessionEntry[], projects: ReadonlyArray<{ projectId: string; name: string }>): ProjectSessionGroup[];
// packages/tddy-web/src/components/sessions/SessionProjectGroup.tsx (new)
export function SessionProjectGroup(props: { group: ProjectSessionGroup; collapsed: boolean; onToggle(): void; children: React.ReactNode }): JSX.Element;
// packages/tddy-web/src/components/sessions/projectGroupCollapse.ts (new)
export function readCollapsedProjects(): Set<string>;
export function writeCollapsedProjects(ids: ReadonlySet<string>): void;
// SessionDrawerProps gains (optional):
projects?: ReadonlyArray<{ projectId: string; name: string }>;
// cypress/support/testIds.ts
sessionsDrawerProjectGroup(projectKey), sessionsDrawerProjectGroupHeader(projectKey) // key = projectId | "other"
```
Plus the failing tests below. Bodies `// TODO(project-groups): implement`.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — the util is pure and the spec mounts the drawer with in-memory sessions and projects.
**Concurrent with:** `feature/project-home/home-agent-setting` (1/7), `feature/project-home/activity-sidebar` (3/7)
**Blocks:** `feature/project-home/home-session` (4/7) — its pinned entry renders inside these groups.

Real dependency edges, as opposed to the branch line:

    n1 home-agent-setting → n4      n2 project-groups → n4      n3 activity-sidebar → n4      n4 home-session → n5, n6      n5 home-session-tools → n7

Waves: 1: n1, n2, n3 · 2: n4 · 3: n5, n6 · 4: n7 — line `1 1 1 2 3 3 4`.

## Affected Packages

- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — drawer grouping

## Related Feature Documentation

- PRD: [PRD-2026-10-22-project-home-project-groups.md](../../ft/web/1-WIP/PRD-2026-10-22-project-home-project-groups.md)

## Summary

Groups the drawer's sessions by `projectId` into collapsible project groups, with a trailing
**Other** group, keeping the active/remaining split and stack nesting inside each group.

## Background

A multi-project drawer is a flat mix today; node 4 needs a per-project place to pin the home entry.

## Scope

- [ ] `groupSessionsByProject` util
- [ ] `SessionProjectGroup` + persisted collapse
- [ ] `SessionDrawer` renders groups; selection auto-expands
- [ ] Tests

## Technical Changes

### State A
`SessionDrawer.tsx` takes a flat `SessionEntry[]`, calls `partitionSessionsByActivity` (active vs
remaining by `connectionStatusForSession`) and renders each partition's `groupSessionsByStack`
result (`utils/sessionStackGroups.ts`). Project names are loaded by `useSessionsDrawerProjects.ts`.

### State B
`SessionDrawer` receives the project list, calls `groupSessionsByProject`, and renders one
`SessionProjectGroup` per group whose body is today's partitioned, stack-grouped list for that
group's sessions.

### Delta
- **tddy-web**: `utils/sessionProjectGroups.ts`, `components/sessions/SessionProjectGroup.tsx`, `components/sessions/projectGroupCollapse.ts` (all new); `SessionDrawer.tsx` and `SessionsDrawerScreen.tsx` pass projects through.

## Testing Plan

Unit tests on the pure util (`bun test`), a Cypress component spec over the mounted drawer, and the
existing drawer specs (`SessionsDrawerAcceptance`, `SessionsDrawerActiveSeparator`,
`SessionsDrawerBulkDeleteAcceptance`, `SessionsDrawerCrossHostAcceptance`) re-run as the regression gate.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `groups sessions under their project's name, ordered by name` | `packages/tddy-web/src/utils/sessionProjectGroups.test.ts` (new) | grouping + order |
| `puts project-less and unknown-project sessions in a trailing Other group` | same | Other last |
| `omits the Other group when every session has a known project` | same | no empty group |
| `the drawer shows one collapsible group per project` | `packages/tddy-web/cypress/component/SessionsDrawerProjectGroupsAcceptance.cy.tsx` (new) | headers + counts |
| `a collapsed project group stays collapsed after a remount` | same | persisted collapse |
| `active and remaining separators appear inside each project group` | same | partition per group |
| `selecting a session in a collapsed group expands it` | same | auto-expand |

## Decisions & Trade-offs
- **Ordered by name**, not by recent activity: activity reorders groups under the cursor on every poll.
- **Collapse persisted in browser storage** — a per-viewer convenience, wrapped so unreadable storage means "all expanded".

## Red-phase notes

- Verified failing (2026-10-23): 3/3 `sessionProjectGroups.test.ts` (`bun test`), 4/4 `SessionsDrawerProjectGroupsAcceptance.cy.tsx` (no group headers rendered).
- Existing drawer specs (`SessionDrawer.cy.tsx`, `SessionsDrawer*Acceptance.cy.tsx`) mount without `projects`; once grouping lands their sessions fall into **Other**. Green re-runs them and adds `projects` where an assertion depends on position.
- `SessionsDrawerScreen` must pass the `ListProjects` rows it already loads (`useSessionsDrawerProjects.ts`) into `projects`.

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-22-project-home-project-groups-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the developer asked for wave 2 to run without per-node review, 2026-10-23)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests for touched packages; full-workspace result from CI
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
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-project-groups-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
