# Changeset: Agent activity right sidebar, default-open for managed sessions

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto field + `.session.yaml` field across tddy-service, tddy-changeset, tddy-session-activity, tddy-agent-launch, plus a tddy-web layout change; RPC acceptance + Cypress
**Stack**: #project-home 3/7. Base: `feature/project-home/project-groups`.
**PR**: [#698](https://github.com/uppin/tddy-coder/pull/698)

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
// packages/tddy-web/src/components/sessions/SessionWithActivitySidebar.tsx (new) — the pane wraps its
// base view in this; it renders the toggle into `toolbar` and docks the transcript to the right
export function SessionWithActivitySidebar(props: { session: SessionEntry; sessionToken: string; client?: Client<typeof ActivityService>; suppressed: boolean; isMobile?: boolean; toolbar(toggle: ReactNode): ReactNode; children: ReactNode }): ReactElement;
// testIds: TEST_IDS.agentActivitySidebar = "agent-activity-sidebar"; agentActivityPage.sidebar()
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

- [x] Persist + list `managed_codebase`
- [x] Sidebar component + state
- [x] Main-pane mount (net ≤ 0 lines in `SessionMainPane.tsx`)
- [x] Tests

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
- **tddy-web**: `SessionWithActivitySidebar.tsx`, `SessionActivitySidebar.tsx`, `activitySidebarState.ts` (new); `SessionMainPane.tsx` swaps mount; `AgentActivityOverlay.tsx` kept, not reduced or removed: no production mount any more (decided in green, see Green-phase notes).

## Testing Plan

Rust: an acceptance test that starts a managed and an unmanaged session through the daemon test
harness and reads `ListSessions`. Web: unit tests for the default rule, Cypress component spec for the
sidebar; existing `AgentActivity*.cy.tsx` specs re-pointed from popover to sidebar where they assert placement.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_managed_session_is_listed_as_managed` | `packages/tddy-session-lifecycle/tests/list_sessions_managed_codebase_acceptance.rs` (new — the stub-claude start harness lives in this crate) | persisted + listed |
| `an_unmanaged_session_is_listed_as_not_managed` | same | default false |
| `a_session_yaml_without_the_key_reads_as_not_managed` | `packages/tddy-changeset/src/session_metadata.rs` (`#[cfg(test)]`) | legacy files (passes already — serde default; regression guard) |
| `defaults the sidebar open for a managed session` / `… closed for an unmanaged session` | `packages/tddy-web/src/components/sessions/activitySidebarState.test.ts` (new) | rule |
| `selecting a managed session shows the activity sidebar on the right` | `packages/tddy-web/cypress/component/SessionActivitySidebarAcceptance.cy.tsx` (new) | default open |
| `selecting an unmanaged session leaves the sidebar closed until toggled` | same | default closed + toggle |
| `no activity sidebar while the Activities view is the main pane` | same | one transcript per pane |

## Decisions & Trade-offs
- **Default from a listed field**, not from client-side guesses (e.g. tool names in the transcript): the placement is a daemon fact.
- **Per-selection override, not persisted**: the default is the point; remembering every toggle would hide it.

## Red-phase notes

- Verified failing (2026-10-23): `a_managed_session_is_listed_as_managed` (reader/ListSessions report `false`), `defaults the sidebar open for a managed session`, Cypress `selecting a managed session…` and `selecting an unmanaged session…` (no sidebar, no toggle). Passing already and kept as guards: the unmanaged/legacy/closed-default cases and `no activity sidebar while the Activities view is the main pane`.
- Adding the field touched every `SessionMetadata` / reader `SessionEntry` / proto `SessionEntry` literal (`managed_codebase: false`); `cargo check --all-targets` clean on tddy-changeset, -session-activity, -agent-launch, -session-lifecycle, -daemon-rpc, -daemon, -coder, -core, -session-agents, -session-files, -session-split, -telegram, -telegram-control, -testing-commons, -integration-tests, -coder-cli.
- The record point is the initial metadata write of every managed start path (claude-cli co-located, sandboxed, split agent half, cursor-cli where accepted); the two read-side TODOs are `session_reader.rs` and `session_coordinate_handlers.rs`.

## Green-phase notes

Green, 2026-10-23. Scoped gates only; full-workspace result comes from CI.

- **Recorded at start.** `managed_codebase` is written into the initial `.session.yaml` on every managed start path: co-located claude-cli (`claude_cli_spawn.rs`, a `managed_codebase` parameter threaded from `cli_branch_starts.rs`), sandboxed claude-cli (`write_jail_session_metadata`), the split agent half (`write_split_agent_metadata`, reads `req`), and co-located and sandboxed cursor-cli. Child and conversation spawns (`child_spawn_handler.rs`, `conversation_spawn_handler.rs`) record `false`. `start_session_core` gained no branch: the flag rides the existing `req.managed_codebase` path.
- **Read and reported.** `session_reader.rs` and `session_coordinate_handlers.rs`, the two read-side TODOs.
- **`AgentActivityOverlay.tsx`: kept, not reduced or removed.** `SessionMainPane` no longer mounts it. Eight component specs still mount it standalone (the seven `AgentActivity*.cy.tsx` specs and `ActivitiesTailScrollAcceptance.cy.tsx`), so removing it means removing or re-mounting those specs, which is beyond the placement re-point and needs the developer's go-ahead. Marked `FIXME(activity-sidebar)`. Developer call: delete it with those specs, or re-mount them on the sidebar.
- **Placement re-points: none needed.** No AgentActivity spec asserts popover placement; they open the transcript through `agentActivityPage.open()` and assert its contents. `InactiveSessionActivitiesAcceptance` asserts only that the toggle exists, which the sidebar's toggle satisfies unchanged.
- **Width, not `useIsMobile`.** `useIsMobile` counts any touch-capable device as mobile, and the Cypress Electron browser reports touch support, so the 1280px spec took the mobile overlay branch. The sidebar decides on width alone via the new `useIsNarrowViewport` (same `md` breakpoint, in `hooks/useIsMobile.ts`); the shared hook is unchanged.
- **Inline layout.** The component specs do not apply the Tailwind flex classes (the sidebar rendered as block layout), so the root, row, content column and dock use inline styles, as the popover did.
- **Toggle and sidebar appear once the session has transcript entries**, as the popover's icon did. A managed session with no activity yet shows no sidebar until its first entry arrives. Developer call: show an empty open sidebar instead?
- **The replay hook stays mounted while `suppressed`.** A hook cannot unmount without remounting `children` (open agent conversations inside them would cancel). While suppressed it keeps the count feed open and marks counted activity seen, so the badge does not carry Activities-view entries into the next view. A subscription trade-off.
- **`SessionMainPane.tsx`: 443 → 438 lines** (`wc -l`). The header row moved into `SessionWithActivitySidebar`'s `toolbar`; the picker and body became its children. The oversized-file record's 674 figure was stale (443 since `b2b9fd1d0`, #663); a measurement row was added and closing the record is left to the wrap.

Verified (scoped): `./dev cargo test -p tddy-session-lifecycle --test list_sessions_managed_codebase_acceptance --test managed_codebase_workflow_acceptance` (2 + 5 passed); `./dev cargo test -p tddy-changeset -p tddy-session-activity` (all passed); `./dev cargo test -p tddy-agent-launch --lib` (127 passed); `./dev cargo clippy -p tddy-changeset -p tddy-session-activity -p tddy-session-lifecycle -p tddy-agent-launch --all-targets -- -D warnings` (clean); `cargo fmt --check` on the five touched Rust packages (clean). Web: `bun test activitySidebarState.test.ts` (2 pass); `SessionActivitySidebarAcceptance.cy.tsx` (3 pass); the AgentActivity regression, one spec at a time: `AgentActivityAcpReplay` 8, `ClientIdentity` 2, `DetailAbsentBody` 4, `DetailDialog` 6, `DetailLazyFetch` 7, `LazyCountPersist` 5, `SnapshotInFlight` 2, `ActivitiesTailScroll` 14, `InactiveSessionActivities` 18 (all pass; its first run failed to load on a Vite dependency-optimizer reload and passed on re-run). Not run locally: other `SessionMainPane` consumers; CI covers them.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the developer asked for wave 2 to run without per-node review, 2026-10-23)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run scoped tests for touched packages; full-workspace result from CI
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
- [x] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-activity-sidebar-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
