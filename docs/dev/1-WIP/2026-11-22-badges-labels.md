# Changeset: Custom badges and labels on work items

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — item-model fields across store, tools and a web row, a proto addition, no I/O beyond the store
**Stack**: `#workqueue` 7/14 — branch `feature/workqueue/badges-labels`, base `feature/workqueue/pr-and-session-link`

## Initial Discovery

[2026-11-22-badges-labels-initial-discovery.md](./2026-11-22-badges-labels-initial-discovery.md)

## Affected Packages

- **tddy-changeset**: `WorkItem.{labels, badges}`, `WorkItemBadge { key, text, tone }`, `BadgeTone`
- **tddy-work-queue**: `set_labels`, `set_badge`, `clear_badge` item ops with limits
- **tddy-service**: `work_queue.proto` `SetWorkItemLabels`, `SetWorkItemBadge`, `ClearWorkItemBadge`; closed lists
- **tddy-tool-engine**: three tool definitions and ops; closed lists
- **tddy-daemon-rpc**: handlers (owner-only writes per n5's access)
- **tddy-tools**, **tddy-agent-launch** prompt: tool lists
- **tddy-workqueue-web**: `WorkItemLabels`, `WorkItemBadges`, label filter in `WorkQueueView`, row edit

## Related Feature Documentation

- PRD: [PRD-2026-11-22-badges-labels.md](../../ft/coder/1-WIP/PRD-2026-11-22-badges-labels.md)

## Summary

Agent- and user-set labels (filterable) and keyed badges (with tone) on work items.

## Background

Exploration 3 §4A.4: `PrInternalStatus` is the only marker; `PlannedPrRowBadges.tsx`
(`INTERNAL_STATUS_BADGE_CLASSES`) renders it.

## Responsibility

- `WorkItem.labels`, `WorkItem.badges`, `WorkItemBadge`, `BadgeTone` and their limits.
- The three tools and RPCs.
- Rendering chips and badges, the label filter, and the row's label edit / badge clear.

## Boundaries

- No derived badges (internal status stays as is).
- No queue-level labels; no label-based access or ordering.
- No activity when a label or badge changes (n8's moves are positional).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n5` session-work-queue (#1006) | session-keyed queue, `WorkQueueAccess` (owner writes) | the new ops are owner writes | change access, storage or creation |
| `n6` pr-and-session-link (#1007) | line predecessor only (item fields `pr`, `linked_sessions`) | — | — |

## Draft PR contract

The first push of this PR, before its implementation: the item fields and types, three tool
definitions, proto RPCs, web component test ids, and the failing tests below.

## Green wave

**Wave:** 4 of 8
**Greenable independently:** yes once n5 is in its base — every test uses only this node's fields and n5's store
**Concurrent with:** n6 pr-and-session-link, n8 move-activities
**Blocks:** nothing

Real dependency edges (the line is n1 · n2 · n3 · … · n14; `a → b` means b needs a's behaviour):

    n1 remove-pr-stack-recipe → n3 workitem-rename
    n2 github-event-inbox     → n4 session-pr-tabs, n6 pr-and-session-link
    n3 workitem-rename        → n5 session-work-queue
    n4 session-pr-tabs        → n11 commit-inbox-outbox
    n5 session-work-queue     → n6 pr-and-session-link, n7 badges-labels, n8 move-activities
    n6 pr-and-session-link    → n9 lineage-graph
    n9 lineage-graph          → n10 waves-line, n11 commit-inbox-outbox, n12 leak-linearity
    n10 waves-line            → n13 schedules
    n11 commit-inbox-outbox   → n13 schedules
    n13 schedules             → n14 land-propagate

Waves: 1 = n1, n2 · 2 = n3, n4 · 3 = n5 · 4 = n6, n7, n8 · 5 = n9 · 6 = n10, n11, n12 · 7 = n13 ·
8 = n14 (run along the line `1 1 2 2 3 4 4 4 5 6 6 6 7 8` — never steps back). Inside each wave the
blockers lead: n1 (12 transitive dependents) before n2 (8); n3 (10) before n4 (3); n6 (6) before n7
and n8 (0 each); n10 and n11 (2 each — tie, kept in plan order) before n12 (0). The sweep node is
reserved and planned after the pipelines drain.

## Scope

- [ ] Item fields with limits
- [ ] Three tools and RPCs
- [ ] Chips, badges, filter, row edit
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

`WorkItem.internal_status: Option<PrInternalStatus>` is the only marker.

### State B (Target)

`WorkItem { …, labels: Vec<String>, badges: Vec<WorkItemBadge> }`, written by owner tools and the UI.

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [ ] Fields + ops + limits
- [ ] Tools + RPCs
- [ ] Web rendering + filter + edits

## Testing Plan

Unit tests on the ops (limits, replace-by-key); integration through the daemon tool path; Cypress
component specs for chips, tones and the filter.

## Acceptance Tests

`packages/tddy-work-queue/tests/work_item_annotations_unit.rs`:
- [ ] `setting_labels_replaces_the_previous_set`
- [ ] `setting_a_badge_with_an_existing_key_replaces_it`
- [ ] `clearing_a_badge_removes_only_that_key`
- [ ] `an_over_limit_label_set_is_refused_naming_the_limit`

`packages/tddy-daemon-rpc/tests/work_item_annotations_acceptance.rs`:
- [ ] `the_owning_agent_labels_and_badges_an_item`
- [ ] `a_linked_session_cannot_badge_an_item`

`packages/tddy-workqueue-web/cypress/component/WorkItemAnnotationsAcceptance.cy.tsx`:
- [ ] `labels show as chips and badges by tone, apart from the internal status badge`
- [ ] `choosing a label filters the queue to items carrying it`

## Decisions & Trade-offs

- **Badges are keyed** so an agent updates one marker (`qa`) without tracking the others.
- **Limits are explicit refusals**, not truncation.
- **No activity on annotation changes** — the operator asked for activities on moves; annotations are
  the agent's own writes.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-badges-labels-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
