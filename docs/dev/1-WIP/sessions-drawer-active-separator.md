# Changeset: sessions-drawer-active-separator

**Type:** Feature
**Packages:** tddy-web
**Product area:** [Session Drawer Screen](../../ft/web/session-drawer.md)

## Summary

Split the open `SessionDrawer` list into an **Active** partition (green/yellow dots) and a
**Remaining** partition (grey/disconnected dots), separated by a collapsible header row labelled
`Active (N)` / `Remaining (M)`. Active is expanded by default; Remaining is collapsed by default.
Existing PR-stack group nesting is preserved *within* each partition. The separator only appears
when both partitions are non-empty (otherwise the list renders flat, as today). Bulk-delete
selection is unchanged and spans both partitions.

## Motivation

The drawer currently renders one flat newest-first list mixing live and finished sessions. As
sessions accumulate, dead/history rows crowd out the ones the operator is actually working with.
Partitioning by liveness (with the finished set collapsed) keeps the working set at the top while
still one click away from the history.

## Decisions (from planning Q&A)

1. **Partition key = each session's own status.** "Active" = green **or** yellow dot, i.e.
   `connectionStatusForSession(entry) !== "disconnected"`. A `needs-input` (yellow) session counts
   as active so a session demanding human input is never hidden in the collapsed partition.
   Sessions are filtered into two lists *first*, then each list is stack-grouped independently.
   A mixed-activity PR-stack therefore splits across partitions by each session's own dot.
2. **Separator shown only when both partitions are non-empty.** All-active or all-disconnected
   lists render as a single flat list with no header — preserving today's layout and avoiding
   an empty-looking collapsed drawer.
3. **Details-style collapse, button toggle.** The header uses a `<button>` disclosure toggle
   (chevron + label + count) with the partition body hidden via `display:none` when collapsed —
   the same *behaviour* as the PR-stack `<details>` group, but not a literal nested `<details>`,
   so it neither conflates with nor inflates the stack-group `<details>` count that existing
   structural assertions rely on.

## Scope

### New

- `utils/sessionStackGroups.ts` — `partitionSessionsByActivity(sessions): { active: SessionStackGroupResult; remaining: SessionStackGroupResult; activeCount: number; remainingCount: number }`. Filters by `connectionStatusForSession(s) !== "disconnected"`, then `groupSessionsByStack` on each partition; counts are the raw partition sizes.
- `components/sessions/SessionDrawerSeparator.tsx` — a collapsible header + partition body. Props: `label`, `count`, `defaultOpen`, `forceOpen` (selection mode), `testId`, `children`. Renders `data-testid={testId}` header text `"{label} ({count})"`; body toggled by `display:none`.

### Modified

- `components/sessions/SessionDrawer.tsx` — replace `groupSessionsByStack(sessions)` with `partitionSessionsByActivity(sessions)`. When both partitions are non-empty, render two `SessionDrawerSeparator` sections (Active default-open, Remaining default-closed), each wrapping that partition's groups + flat rows; else render the single non-empty partition's rows flat with no separator. Pass `forceOpen={selectionMode}` to both separators. Extract the existing "groups + flat" render into a shared helper. Bulk-delete/select-all logic and props unchanged.
- `cypress/support/testIds.ts` — add `sessionsDrawerSeparatorActive` = `"sessions-drawer-separator-active"` and `sessionsDrawerSeparatorRemaining` = `"sessions-drawer-separator-remaining"`.

## Testing Plan

**Test level:** Unit (bun) + Component acceptance (Cypress). No backend/proto change → no Rust.

### Acceptance tests — `cypress/component/SessionsDrawerActiveSeparator.cy.tsx` (new)

Mounts `SessionDrawer` directly (like `SessionDrawer.cy.tsx`), fluent-tests style.

- labels the partitions `Active (N)` and `Remaining (M)` with live counts (green + yellow + grey → `Active (2)` / `Remaining (1)`)
- shows the active partition expanded by default (green row visible)
- collapses the remaining partition by default (grey row not visible)
- expands the remaining partition when its separator is clicked (grey row becomes visible)
- collapses the active partition when its separator is clicked (green row hidden)
- places a needs-input (yellow) session in the active partition
- renders a plain list with no separators when every session is active
- keeps remaining-partition checkboxes reachable during bulk selection (selection mode force-expands; clicking the collapsed-partition row's checkbox fires `onToggleSelect`)

### Unit tests — `src/utils/sessionStackGroups.test.ts` (extend)

`describe("partitionSessionsByActivity")`:

- returns empty active and remaining partitions for an empty list
- routes a connected (green) session to active and a disconnected (grey) session to remaining
- routes a needs-input (yellow) session to the active partition
- counts each partition by the number of sessions it contains
- preserves stack-group nesting within the active partition
- preserves stack-group nesting within the remaining partition
- splits a mixed-activity stack across partitions by each session's own status

## Implementation status — COMPLETE

Files: `utils/sessionStackGroups.ts` (`partitionSessionsByActivity` + `SessionActivityPartitions`),
new `components/sessions/SessionDrawerSeparator.tsx` (button-based disclosure), and
`components/sessions/SessionDrawer.tsx` (extracted `SessionPartitionBody`; two separators when both
partitions non-empty, else a flat body). testIds + `sessionsDrawerPage.expandRemaining()` helper added.

Verified (nix daemon down → store bun + cached cypress 14.5.4):
- Unit `sessionStackGroups.test.ts`: **14/14** (8 new `partitionSessionsByActivity` cases).
- Acceptance `SessionsDrawerActiveSeparator.cy.tsx`: **8/8**.
- Regression: `SessionDrawer.cy.tsx` 7/7, `SessionsDrawerBulkDeleteAcceptance` 2/2,
  `SessionInspectorAcceptance` 13/13, `SessionInspectorDockedDisconnected` 5/5,
  `SessionParticipantRpcRouting` 3/3, `SessionsDrawerAcceptance` 23/23, `SessionsDrawerCrossHostAcceptance` 8/8.

## Test adaptations (green phase)

The mixed-list risk materialised in exactly two pre-existing specs that click a disconnected row
now defaulting into the collapsed Remaining partition. Both were adapted to the new UI contract by
calling the new `sessionsDrawerPage.expandRemaining()` before the click (no assertion weakened):
- `SessionInspectorAcceptance.cy.tsx` — "opens the inspector when a connected session attachment becomes idle" (mixed `[CONNECTED, DISCONNECTED]` fixture).
- `SessionStartResumeStillDaemonRouted.cy.tsx` — "routes ResumeSession…" (mixed `[ACTIVE, DISCONNECTED]` fixture).

A literal + helper-based grep found no other mixed-fixture drawer specs; all-inactive / single-session
fixtures render flat and pass unchanged. `SessionDrawer.cy.tsx`'s `<details>`-count assertions are
unaffected because the separator is a `<button>`, not a `<details>`.

## Pre-existing failure (NOT a regression)

`SessionStartResumeStillDaemonRouted` → "routes DeleteSession to the daemon participant identity"
fails deterministically in the local headless runner on an RPC-routing assertion (line ~148). It
clicks the **active** (visible) row, so partitioning is not involved. Confirmed by stashing this
changeset and re-running against the original `SessionDrawer`: it fails identically. Documented in
the `session-start-resume-daemon-routed-flaky` memory note.

## Validation Results (pr-wrap)

- **validate-changes:** Critical 0 / Warning 0 / Info 1 (cosmetic: separator chevron reflects toggle state, not `forceOpen` visibility during bulk-selection — no fix). Correctness, docs alignment, security all clean.
- **validate-tests:** Critical 0 / Warning 0. Fluent-compliant (driver + named helpers, Given/When/Then, semantic fixtures, no `cy.intercept`, no branching). Two mixed-fixture specs adapted via `expandRemaining()` with no weakened assertions.
- **validate-prod-ready:** Blockers 0 / Warnings 0. No TODO/FIXME/console/dead code/fallbacks/mocks in production paths.
- **analyze-clean-code:** Score **A**. No must-refactor items; extraction of `SessionPartitionBody` reduced duplication.
- **Lint/typecheck:** web-only change (no Rust → `cargo` N/A). `tsc --noEmit` clean on the three touched source files (only pre-existing repo-wide `bun:test` type noise remains). No eslint config in the package.
- **Tests:** unit 14/14, acceptance 8/8, seven regression suites green. One pre-existing `DeleteSession`-routing flake (base-branch, unrelated) documented above.

No `refactor` passes were required in any step.

## Acceptance criteria

- [x] Open drawer with a mix of live and disconnected sessions shows `Active (N)` and `Remaining (M)` collapsible headers with correct counts
- [x] Active partition expanded by default; Remaining collapsed by default; each toggles independently
- [x] Green and yellow dots land in Active; grey dots land in Remaining
- [x] Stack-group nesting preserved within each partition; mixed stacks split by each session's own dot
- [x] No separator when all sessions share one partition (flat list, as before)
- [x] Bulk select-all + delete operates across both partitions unchanged
- [x] `partitionSessionsByActivity` unit tests pass; acceptance tests pass; no Rust/proto change
