# Changeset: A work item takes an existing PR and linked sessions

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — re-scored in pass 2: more than one crate and two web packages, proto additions, a stream fed by n2's subscriptions, **plus** the two migrations moved here from n2 (repo → project credential plumbing into the queue RPC handler, a sync → async rewrite of the status path)
**Stack**: `#workqueue` 6/14 — branch `feature/workqueue/pr-and-session-link`, base `feature/workqueue/session-work-queue`
**PR**: [#1007](https://github.com/uppin/tddy-coder/pull/1007)

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
- **Moved here from n2 by the operator (2026-11-22):**
  - `pr_status_for_caller` (daemon-rpc; login-keyed token, `repo_root` only) reads PR status through the
    inbox — `tddy_pr_stack::inbox_reads::pr_status_view_through_inbox` — which needs repo → project
    credential plumbing into the queue RPC handler (`PrStackRpcHandler`; the inbox's `RepoReaders`
    registered per project the way the trunk watch's `HostRepoReaders` is).
  - The queue status path (`work_queue_status` / `pr_stack_status` → `assess::assemble_views` over the
    synchronous `GithubPrApi`) reads through the inbox — `inbox_reads::assemble_views_through_inbox`, an
    async rewrite, not a blocking adapter.
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
| `n4` session-pr-tabs (#1005) | `CheckRow` (web, in `tddy-workspace-web`) | reused for expanded checks (`tddy-prstack-web` adds a dependency on `tddy-workspace-web`) | change it |
| `n5` session-work-queue (#1006) | session-keyed queue, `WorkQueueAccess`, addressing by session | new ops follow n5's access; linked sessions join `Addressed` | change the store key, creation or the refusal |

**Sequencing (pass 2):** `inbox_reads_acceptance.rs` keeps n2's real inbox (reading through it is the
point; n2 is green). `a_session_linked_to_an_item_is_addressed_by_its_queue` calls n5's
`work_queue_access_of` and so goes green only after n5 is green.

## Draft PR contract

Published (pass 2, commit 2):

- Proto (`pr_stack.proto`, renamed by n3): `AssignWorkItemPr`, `UnassignWorkItemPr`, `LinkWorkItemSession`,
  `UnlinkWorkItemSession` (→ `WorkItemLinksResponse`); `PrStackHandler` gains them with default bodies
  answering `unimplemented`, the service adapter and the test daemon forward them; TS regenerated.
- `tddy_pr_stack::stack_ops::item_links::{AssignedPr, LinkedSession, ItemLinkError, assign_pr, unassign_pr,
  link_session, unlink_session, assigned_pr_of, linked_sessions_of}` — pure operations on a queue.
- `tddy_pr_stack::inbox_reads::{pr_status_view_through_inbox, assemble_views_through_inbox,
  item_check_summaries, ItemCheckSummary}`.
- `tddy_tool_engine::work_item_link_tools::{WORK_ITEM_ASSIGN_PR, WORK_ITEM_UNASSIGN_PR,
  WORK_ITEM_LINK_SESSION, WORK_ITEM_UNLINK_SESSION, WORK_ITEM_LINK_TOOL_NAMES, work_item_link_tool_defs}`.
- `tddy-prstack-web`: `WorkItemChecksSummary` (shell).
- The failing tests below.

Not in the contract, `/green`'s: the item fields themselves (~80 struct literals across ~30 files
construct `StackNode` without `..Default`, many of them n1's and n3's — the accessors above are the
published way to read links); `StreamWorkQueueStatus` (a server stream needs an associated stream type in
two implementors); the credential plumbing into the RPC handler.

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

`packages/tddy-pr-stack/tests/item_links_unit.rs`:
- [x] `assigning_an_existing_pr_records_it_and_its_branch_on_the_item`
- [x] `assigning_a_pr_on_another_branch_than_the_items_is_refused_naming_both`
- [x] `unassigning_removes_the_pr_and_keeps_the_branch`
- [x] `linking_a_session_twice_links_it_once`
- [x] `unlinking_removes_only_the_named_session`
- [x] `a_link_on_an_unknown_item_is_refused`

`packages/tddy-pr-stack/tests/inbox_reads_acceptance.rs` (a real inbox over a scripted GitHub that counts requests):
- [x] `a_second_reader_of_a_branchs_pr_status_makes_no_github_request` — pins the `pr_status_for_caller` migration
- [x] `the_queue_status_reads_each_items_pr_through_the_inbox_once` — pins the status-path migration
- [x] `a_prd_item_summarises_its_running_failed_and_succeeded_checks`

`packages/tddy-tool-engine/tests/work_item_link_tools.rs`:
- [x] `the_link_tools_are_defined_in_order`
- [x] `the_link_tools_take_an_item_and_a_pr_or_a_session`
- [x] `a_session_linked_to_an_item_is_addressed_by_its_queue` — extends n5's `work_queue_access_of`

The daemon tool and RPC paths (owner-only writes, the handler using the inbox with the project's
credential) are `/green`'s to prove in the daemon-rpc stack suites.

`packages/tddy-workqueue-web/cypress/component/WorkItemLinksAcceptance.cy.tsx`:
- [x] `the user assigns a PR to an item from its row`
- [x] `the user links a session to an item from its row`
- [x] `a PR'ed item shows running, failed and succeeded check counts`

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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
