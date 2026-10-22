# Changeset: Agent activity right sidebar, default-open for managed sessions

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto field + `.session.yaml` field across tddy-service, tddy-changeset, tddy-session-activity, tddy-agent-launch, plus a tddy-web layout change; RPC acceptance + Cypress
**Stack**: #project-home 3/7. Base: `feature/project-home/project-groups`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-22-project-home-activity-sidebar-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [`packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md`](../../../packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md) | ⚠ DURING | Unclaimed, 674/500. The sidebar and its state live in new modules; `SessionMainPane.tsx` only swaps the overlay mount for the sidebar mount. Net growth must be ≤ 0. |
| [`packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md) | ⚠ DURING | Record the placement through the existing metadata write, not a new branch in `start_session_core`. |
| [`2026-08-01-tddy-web-dead-session-surfaces.md`](../todo/2026-08-01-tddy-web-dead-session-surfaces.md) | — | Unrelated: the sidebar reads `useAcpReplay`, not the callerless `useSessionActivity`. |

## Responsibility

`SessionEntry.managed_codebase` end to end (written at start into `.session.yaml`, read by the
session reader, enriched into `ListSessions`), and the right **activity sidebar**: component, the
default-open rule (`managed_codebase`), the per-selection override, and its mount in the session
main pane in place of the popover.

## Boundaries

- Does not change transcript content, paging or the detail dialog.
- Does not add `session_role` or anything project-home specific.
- Does not touch the Activities view or the inspector's contents.
- Does not change how managed sessions are started — only records that they were.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/project-home/home-agent-setting` (1/7) | `home_agent` setting | nothing | read `home_agent` |
| `feature/project-home/project-groups` (2/7) | project groups in the drawer | nothing — position only | change the drawer |

## Draft PR contract

```proto
// session.proto
message SessionEntry { /* … */ bool managed_codebase = 34; }
```
```rust
// tddy-changeset::session_metadata
pub struct SessionMetadata { /* … */ #[serde(default, skip_serializing_if = "is_false")] pub managed_codebase: bool }
// tddy-session-activity::session_reader
pub struct SessionEntry { /* … */ pub managed_codebase: bool }
```
```ts
// packages/tddy-web/src/components/sessions/activitySidebarState.ts (new)
export function defaultActivitySidebarOpen(session: SessionEntry): boolean;
// packages/tddy-web/src/components/sessions/SessionActivitySidebar.tsx (new)
export function SessionActivitySidebar(props: { sessionId: string; sessionToken: string; client?: Client<typeof ActivityService>; onClose(): void }): JSX.Element;
```
Plus the failing tests below. Bodies `// TODO(activity-sidebar): implement`.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — the Rust tests start sessions through the existing in-process harness; the Cypress spec mounts the main pane with in-memory sessions and an in-memory activity backend.
**Concurrent with:** `feature/project-home/home-agent-setting` (1/7), `feature/project-home/project-groups` (2/7)
**Blocks:** `feature/project-home/home-session` (4/7) — its acceptance test asserts the home session opens with the sidebar, which needs `managed_codebase` actually recorded and listed.

Real dependency edges, as opposed to the branch line:

    n1 home-agent-setting → n4      n2 project-groups → n4      n3 activity-sidebar → n4      n4 home-session → n5, n6      n5 home-session-tools → n7

Waves: 1: n1, n2, n3 · 2: n4 · 3: n5, n6 · 4: n7 — line `1 1 1 2 3 3 4`.

## Affected Packages

- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `SessionEntry.managed_codebase`
- **tddy-changeset**: [README.md](../../../packages/tddy-changeset/README.md) — `SessionMetadata.managed_codebase`
- **tddy-session-activity**: [README.md](../../../packages/tddy-session-activity/README.md) — reader + enrichment
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) — start path records placement
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — sidebar

## Related Feature Documentation

- PRD: [PRD-2026-10-22-project-home-activity-sidebar.md](../../ft/web/1-WIP/PRD-2026-10-22-project-home-activity-sidebar.md)

## Summary

Persists and lists whether a session is managed, and turns the Agent Activity popover into a right
sidebar that is open by default for managed sessions.

## Background

The web has no way to tell a managed session; the transcript is the readable view of a managed agent.

## Scope

- [ ] Persist + list `managed_codebase`
- [ ] Sidebar component + state
- [ ] Main-pane mount (net ≤ 0 lines in `SessionMainPane.tsx`)
- [ ] Tests

## Technical Changes

### State A
`managed_codebase` reaches `classify_start_placement` (`svc_start_session_core.rs:470`) and the CLI
branch starts (`cli_branch_starts.rs`) and is then dropped: not in `SessionMetadata`
(`tddy-changeset/src/session_metadata.rs`), not in `session_reader::SessionEntry`, not in the proto
`SessionEntry` (built at `session_list_enrichment.rs:478`). The transcript is `AgentActivityOverlay`, a
popover in the main-pane header (`SessionMainPane.tsx:279`), suppressed while `baseViewMode === "activities"`.

### State B
Managed starts write `managed_codebase: true`; `ListSessions` reports it; the header toggle opens a
`SessionActivitySidebar` docked right, defaulting to `defaultActivitySidebarOpen(session)` on selection.

### Delta
- **tddy-service**: field 34.
- **tddy-changeset**: field + `is_false` skip.
- **tddy-session-activity**: reader field, enrichment copy.
- **tddy-agent-launch**: set the field in the initial metadata write for managed starts (claude-cli, sandboxed, split, cursor-cli where accepted).
- **tddy-web**: `SessionActivitySidebar.tsx`, `activitySidebarState.ts` (new); `SessionMainPane.tsx` swaps mount; `AgentActivityOverlay.tsx` reduced to the toggle button or removed (decide in green; record here).

## Testing Plan

Rust: an acceptance test that starts a managed and an unmanaged session through the daemon test
harness and reads `ListSessions`. Web: unit tests for the default rule, Cypress component spec for the
sidebar; existing `AgentActivity*.cy.tsx` specs re-pointed from popover to sidebar where they assert placement.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_managed_session_is_listed_as_managed` | `packages/tddy-daemon-rpc/tests/list_sessions_managed_codebase_acceptance.rs` (new) | persisted + listed |
| `an_unmanaged_session_is_listed_as_not_managed` | same | default false |
| `a_session_yaml_without_the_key_reads_as_not_managed` | `packages/tddy-changeset/src/session_metadata.rs` (`#[cfg(test)]`) | legacy files |
| `defaults the sidebar open for a managed session and closed otherwise` | `packages/tddy-web/src/components/sessions/activitySidebarState.test.ts` (new) | rule |
| `selecting a managed session shows the activity sidebar on the right` | `packages/tddy-web/cypress/component/SessionActivitySidebarAcceptance.cy.tsx` (new) | default open |
| `selecting an unmanaged session leaves the sidebar closed until toggled` | same | default closed + toggle |
| `no activity sidebar while the Activities view is the main pane` | same | one transcript per pane |

## Decisions & Trade-offs
- **Default from a listed field**, not from client-side guesses (e.g. tool names in the transcript): the placement is a daemon fact.
- **Per-selection override, not persisted**: the default is the point; remembering every toggle would hide it.

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

- [x] Record initial discovery (`2026-10-22-project-home-activity-sidebar-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-activity-sidebar-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
