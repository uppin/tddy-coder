# Changeset: A work item takes an existing PR and linked sessions

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate and two web packages, proto additions (four RPCs, one stream), a stream fed by n2's subscriptions
**Stack**: `#workqueue` 6/14 — branch `feature/workqueue/pr-and-session-link`, base `feature/workqueue/session-work-queue`

## Initial Discovery

[2026-11-22-pr-and-session-link-initial-discovery.md](./2026-11-22-pr-and-session-link-initial-discovery.md)

## Affected Packages

- **tddy-changeset**: `WorkItem.{pr, linked_sessions}`
- **tddy-work-queue**: `assign_pr`, `unassign_pr`, `link_session`, `unlink_session` item ops; `pull_number_for_item` prefers the assigned number
- **tddy-service**: `work_queue.proto` `AssignWorkItemPr`, `UnassignWorkItemPr`, `LinkWorkItemSession`, `UnlinkWorkItemSession`, `StreamWorkQueueStatus`; closed lists
- **tddy-tool-engine**: four `work_item_*` tool definitions and ops; closed lists
- **tddy-daemon-rpc**: handlers; the status stream over the inbox; access counts linked sessions as addressed
- **tddy-tools**: admission list; **tddy-agent-launch**: prompt tool list
- **tddy-workqueue-web**: row controls (assign, link), `WorkItemChecksSummary`, expanded check rows reusing n4's `CheckRow`

## Related Feature Documentation

- PRD: [PRD-2026-11-22-pr-and-session-link.md](../../ft/coder/1-WIP/PRD-2026-11-22-pr-and-session-link.md)

## Summary

Assign/unassign a PR and link/unlink sessions on a work item (tools, RPCs, UI); PR'ed items show live
checks from the inbox.

## Background

Exploration 3 §4A.4: one `session_id` per item, no PR number, PR recovered from a URL. Exploration 2
§4A: check runs are read only by agent calls.

## Responsibility

- `WorkItem.pr`, `WorkItem.linked_sessions`; the four operations as tools and RPCs.
- `StreamWorkQueueStatus` — per item: PR state and check summary, pushed on inbox events for the
  queue's repository (kinds `Pr`, `Checks`).
- Counting linked sessions in n5's `Addressed` access.
- `WorkItemChecksSummary` and the row controls.

## Boundaries

- No lineage edges from the assigned PR's base (n9).
- No comments or commits on the row (the PR view, n4/n11).
- No GitHub mutation (assigning never edits the PR).
- Does not change the inbox, its rows or kinds (n2).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` github-event-inbox (#1003) | `GithubInbox::{subscribe, pr, checks_at}`, `EventKind::{Pr, Checks}` | the status stream subscribes and reads | call GitHub directly or cache anything itself |
| `n4` session-pr-tabs (#1005) | `CheckRow` (web) | reused for expanded checks | change it |
| `n5` session-work-queue (#1006) | session-keyed queue, `WorkQueueAccess`, addressing by session | new ops follow n5's access; linked sessions join `Addressed` | change the store key, creation or the refusal |

## Draft PR contract

The first push of this PR, before its implementation: the two item fields, four tool definitions,
proto RPCs and stream, item-op signatures, web component test ids, and the failing tests below.

## Green wave

**Wave:** 4 of 8
**Greenable independently:** no — the live-checks test needs n2's inbox behaviour and every test needs n5's session queue
**Concurrent with:** n7 badges-labels, n8 move-activities
**Blocks:** n9 lineage-graph (line edges are read from assigned PRs' base branches)

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

## Prerequisites

### ⚠ DURING — [oversized-file-pr-stack-ports.md](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md)

The new handlers go in their own module beside `work_queue/ports.rs`, which must not grow.

### ℹ ANSWERED — [2026-07-26-pr-stack-status-polling-and-stack-hygiene.md](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

"Dangling `session_id`": an unlinked or deleted linked session is dropped from `linked_sessions` by
`work_item_unlink_session`; an automatic scrub on session delete stays open.

## Scope

- [ ] Item fields + four ops (tools, RPCs, UI)
- [ ] Live check summary over the inbox
- [ ] Linked sessions are addressed for access
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

`WorkItem { …, branch, session_id, pr_status, … }`; `pull_number_for_node` parses `pr_status.url`;
`work_item_adopt` creates a new item from a PR.

### State B (Target)

`WorkItem { …, pr: Option<AssignedPr>, linked_sessions: Vec<LinkedSession> }`; a queue status stream
per open view, fed by inbox subscriptions.

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [ ] Item fields + ops + validation (branch mismatch)
- [ ] Tools + RPCs + access
- [ ] Status stream over the inbox
- [ ] Web controls + checks summary

## Testing Plan

Integration tests through the daemon tool path and RPCs (in-process); the stream over an inbox backed
by a scripted GitHub; Cypress component specs for the row.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/work_item_links_acceptance.rs`:
- [ ] `the_agent_assigns_an_existing_pr_to_an_item`
- [ ] `assigning_a_pr_on_another_branch_than_the_items_is_refused_naming_both`
- [ ] `a_linked_session_reads_the_queue_of_its_item`
- [ ] `unlinking_removes_only_the_named_session`
- [ ] `the_status_stream_pushes_a_completed_check_for_a_prd_item`

`packages/tddy-workqueue-web/cypress/component/WorkItemLinksAcceptance.cy.tsx`:
- [ ] `the user assigns a PR to an item from its row`
- [ ] `the user links a session to an item from its row`
- [ ] `a PR'ed item shows running, failed and succeeded check counts`

## Decisions & Trade-offs

- **The assigned PR's branch becomes the item's branch**; a conflicting recorded branch is refused
  rather than overwritten (no silent fallback).
- **Linked sessions are read-only addressees** of the queue (n5's access); writing stays the owner's.
- **One status stream per open queue view**, fanned out from inbox subscriptions — never per row.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-pr-and-session-link-initial-discovery.md`)
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
