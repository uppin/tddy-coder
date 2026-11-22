# Changeset: Leaked commits and non-linear items

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — two pure checks with unit tests, inputs over existing seams (inbox, git), status fields and two chips
**Stack**: `#workqueue` 12/14 — branch `feature/workqueue/leak-linearity`, base `feature/workqueue/commit-inbox-outbox`

## Initial Discovery

[2026-11-22-leak-linearity-initial-discovery.md](./2026-11-22-leak-linearity-initial-discovery.md)

## Affected Packages

- **tddy-work-queue**: `chain_health` module — `leaks`, `Leak { sha, ancestor }`, `line_state`, `LineState::{Linear, Behind{by}, Conflicting{by}, Unknown}`
- **tddy-service**: `work_queue.proto` `WorkItemStatus.{leaks, line_state}`
- **tddy-daemon-rpc**: status assembly (PR commits via the inbox for the item and its line ancestors; containment via git)
- **tddy-tool-engine**: status payload
- **tddy-workqueue-web**: `LeakChip`, `LinearityChip`, leak detail

## Related Feature Documentation

- PRD: [PRD-2026-11-22-leak-linearity.md](../../ft/coder/1-WIP/PRD-2026-11-22-leak-linearity.md)

## Summary

Per-item leak detection (SHA or rebased-copy match against line ancestors' PR commits) and a linearity
verdict, on rows and in status.

## Background

Exploration 3 §4E (git helpers) and Exploration 2 §2A (`GithubPrInsightApi`; n2's inbox serves
`pr_commits` and `pr` with `mergeable`).

## Responsibility

- `chain_health::{leaks, line_state}` and their types.
- Assembling their inputs in status; the two chips.

## Boundaries

- No repair, no rebase, no PR edit.
- No new inbox row kind: PR commits and `mergeable` are rows n2 already serves; containment is read
  with git, not with a GitHub compare.
- No new tool: both verdicts ride on `work_queue_status`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` github-event-inbox (#1003) | `GithubInbox::{pr_commits, pr}` (incl. `mergeable`) | leak inputs and the conflicting verdict | add a compare row or read GitHub directly |
| `n9` lineage-graph (#1010) | `line_parent_of`, line ancestors | which ancestors to compare against; which parent tip | compute line edges |
| `n11` commit-inbox-outbox (#1012) | git remote-tip reads | containment of the parent tip | change `commit_flow` |

## Draft PR contract

The first push of this PR, before its implementation: `chain_health`'s signatures and types, status
fields, chip test ids, and the failing tests below.

## Green wave

**Wave:** 6 of 8
**Greenable independently:** no — line ancestors come from n9; PR commits from n2's inbox
**Concurrent with:** n10 waves-line, n11 commit-inbox-outbox
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

- [ ] `chain_health` checks
- [ ] Status fields
- [ ] Chips and leak detail
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

No leak or linearity check anywhere.

### State B (Target)

`leaks(own, ancestors_nearest_first) -> Vec<Leak>`; `line_state(contains_parent_tip, behind_by, mergeable) -> LineState`.

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [ ] Pure checks with unit tests
- [ ] Status assembly
- [ ] Chips

## Testing Plan

Unit tests for both checks (SHA match, rebased-copy match, nearest ancestor, each line state);
one daemon status test with an inbox over a scripted GitHub and a temp repo; Cypress chips.

## Acceptance Tests

`packages/tddy-work-queue/tests/chain_health_unit.rs`:
- [ ] `a_commit_with_an_ancestors_sha_is_leaked_from_it`
- [ ] `a_rebased_copy_with_the_same_author_date_and_subject_is_leaked`
- [ ] `the_nearest_ancestor_carrying_a_commit_is_named`
- [ ] `an_item_containing_its_parents_tip_is_linear`
- [ ] `an_item_missing_parent_commits_is_behind_by_their_count`
- [ ] `behind_and_unmergeable_is_conflicting`
- [ ] `a_merged_or_planned_parent_gives_unknown`

`packages/tddy-daemon-rpc/tests/work_queue_chain_health_acceptance.rs`:
- [ ] `work_queue_status_carries_leaks_and_line_state`

`packages/tddy-workqueue-web/cypress/component/WorkItemChainHealthAcceptance.cy.tsx`:
- [ ] `rows show a leak chip naming the ancestor and a linearity chip`

## Decisions & Trade-offs

- **Containment from git, not a GitHub compare** — keeps n2's inbox surface unchanged (no row kind
  added outside its owner) and reuses n11's remote-tip reads.
- **Rebased copies count as leaks** (author + author date + subject) — a rebase changes the SHA but
  not the fact that the work belongs to the ancestor.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-leak-linearity-initial-discovery.md`)
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
