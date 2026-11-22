# Changeset: Commit inbox and outbox with conflict prediction

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — git subprocess adapter seam with temp-repo integration tests, a cache, more than one crate and a web extension, proto additions
**Stack**: `#workqueue` 11/14 — branch `feature/workqueue/commit-inbox-outbox`, base `feature/workqueue/waves-line`
**PR**: [#1012](https://github.com/uppin/tddy-coder/pull/1012)

## Initial Discovery

[2026-11-22-commit-inbox-outbox-initial-discovery.md](./2026-11-22-commit-inbox-outbox-initial-discovery.md)

## Affected Packages

- **tddy-work-queue**: `commit_flow` module — `own_commits`, `inbox`, `outbox`, `InboxEntry { commit, source, prediction }`, `Prediction::{Clean, Conflicts(files)}`; `CommitFlowCache` keyed by tip SHAs
- **tddy-worktree-service** (or the crate's git helpers, wherever n1 left them): `commits_in_range` (no merges, oldest first), `merge_tree_prediction(ours, theirs)` with explicit merge-base, remote-tip reads
- **tddy-service**: `work_queue.proto` `GetWorkItemInbox`, `GetWorkItemOutbox`; row counts; closed lists
- **tddy-tool-engine**: `work_item_inbox`, `work_item_outbox`; closed lists
- **tddy-daemon-rpc**: handlers; which clone the analysis runs in (the queue owner's worktree, fetch only)
- **tddy-tools**, **tddy-agent-launch** prompt: tool lists
- **tddy-session-web** / **tddy-workqueue-web**: Inbox and Outbox tabs in n4's PR view; row counts and conflict marker

## Related Feature Documentation

- PRD: [PRD-2026-11-22-commit-inbox-outbox.md](../../ft/coder/1-WIP/PRD-2026-11-22-commit-inbox-outbox.md)

## Summary

Own commits, attributed inbox with per-item merge-tree predictions, and outbox per successor — read
from remote tips, cached by SHA, shown in the PR view and returned by two tools.

## Background

Exploration 3 §4E: `base_sync::count_divergence`, `conflicting_paths` (`merge-tree --write-tree
--name-only -z`), `subagent-worktree/sync.rs::merge_tree` (explicit merge-base), `lineage.rs`
(first-parent listing), `base_sync_cache` (SHA-keyed). None attributes commits to ancestors or predicts
per commit.

## Responsibility

- `commit_flow` (own commits, inbox, outbox, predictions, attribution) and its cache.
- The git reads it needs, if not already present.
- `work_item_inbox`, `work_item_outbox`, their RPCs; row counts.
- The PR view's Inbox and Outbox tabs.

## Boundaries

- No propagation or rebase (n14); no schedule conditions (n13 reads this).
- No leak or linearity verdicts (n12).
- No change to `QueryBranch` or `base_sync`.
- Never touches a local branch, index or working tree; fetch updates remote-tracking refs only.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n4` session-pr-tabs (#1005) | the PR view (`SessionPullRequestTab`), `CommitList` | adds Inbox and Outbox tabs beside Commits | restructure the PR view or change its stream |
| `n9` lineage-graph (#1010) | `line_parent_of`, `LineEndpoint` | frames every range (`parent..item`, successors) | compute line edges itself |
| `n10` waves-line (#1011) | line predecessor only | — | — |

## Draft PR contract

Published (pass 2, commit 2):

- `tddy_pr_stack::commit_flow::{CommitFlow (new, fetch, own_commits, inbox, outbox, cache_hits),
  ItemBranch, FlowCommit, CommitSource, Prediction, InboxEntry, OutboxEntry, CommitFlowError}` — over
  items already resolved with their line parent (n9's `LineEndpoint`), so the tests need no GitHub
- `tddy_tool_engine::work_item_commit_flow_tools::{WORK_ITEM_INBOX, WORK_ITEM_OUTBOX,
  work_item_commit_flow_tool_defs}`
- `tddy-workspace-web`: `PullRequestInboxTab`, `PullRequestOutboxTab` (shells, props-driven; `/green`
  mounts them in n4's PR view — n4's component is not touched here)
- The PRD's reference to n4 is by branch and PR (it linked n4's PRD by path in pass 1)
- The failing tests below. The `GetWorkItemInbox` / `GetWorkItemOutbox` RPCs and row counts are `/green`'s.

`inbox(item)` covers **every** line ancestor (`⋃ origin/<item>..origin/<a>`), not just the parent: a
commit pushed to n1 is in n3's inbox before n2 has taken it.

## Green wave

**Wave:** 6 of 8
**Greenable independently:** no — needs n9's line parents (behaviour) and n4's PR view
**Concurrent with:** n10 waves-line, n12 leak-linearity
**Blocks:** n13 schedules (merge `current` and propagation `inbox` / `conflicts` conditions)

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

### ⚠ DURING — `tddy-pr-stack/src/git_ops.rs` (renamed by n3) at 468 production lines

New git reads go in their own module.

### Related — [2026-08-30-pr-stack-querybranch-s-local-only-legs.md](../todo/2026-08-30-pr-stack-querybranch-s-local-only-legs.md)

Analysis runs on the queue owner's host from remote tips, so it does not depend on which host holds an
item's worktree.

## Scope

- [ ] `commit_flow` with attribution and per-item predictions
- [ ] SHA-keyed cache
- [ ] Two tools + RPCs + row counts
- [ ] PR view Inbox / Outbox tabs
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

Behind/ahead counts and conflicting paths against one named base (`base_sync`); no attribution, no
per-commit prediction, no outbox.

### State B (Target)

```
own(item)     = origin/<line_parent>..origin/<item>
inbox(item)   = origin/<item>..origin/<line_parent>, each commit c:
                  source = nearest line ancestor whose branch contains c, else the base endpoint
                  prediction = merge-tree(origin/<item>, c) with explicit merge-base
outbox(item)  = ⋃ successors s: inbox(s) entries sourced from item
cache key     = (item tip, parent tip)
```

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [ ] Git reads + prediction against temp repos
- [ ] `commit_flow` + cache
- [ ] Tools + RPCs + row counts
- [ ] PR view tabs

## Testing Plan

Integration tests on temp repositories with a bare `origin` and scripted pushes to ancestors (clean and
conflicting replays); a no-mutation assertion (refs, index, worktree unchanged); a cache-hit count; a
daemon tool test; Cypress component spec for the tabs.

## Acceptance Tests

`packages/tddy-pr-stack/tests/commit_flow_acceptance.rs` (real repos; crate renamed by n3):
- [x] `a_commit_pushed_to_an_ancestor_after_the_cut_is_in_the_items_inbox_attributed_to_it`
- [x] `an_items_outbox_equals_its_successors_inbox_entries_from_it`
- [x] `one_commit_can_be_clean_for_one_item_and_conflicting_for_the_next`
- [x] `an_items_own_commits_are_those_beyond_its_line_parent` (the conflicting prediction naming the files is asserted in `one_commit_can_be_clean…`)
- [x] `trunk_commits_are_an_external_source_listed_first`
- [x] `the_analysis_changes_no_local_branch_index_or_worktree`
- [x] `unchanged_tips_answer_from_the_cache`

`packages/tddy-tool-engine/tests/work_item_commit_flow_tools.rs`:
- [x] `the_inbox_and_outbox_tools_each_take_an_item`

`packages/tddy-workspace-web/cypress/component/PullRequestInboxOutboxAcceptance.cy.tsx`:
- [x] `the inbox lists each commit with its source and a conflict marker naming the files`
- [x] `the outbox lists, per successor, what it lacks`

The tool's daemon path is `/green`'s, in the daemon-rpc stack suites.

## Decisions & Trade-offs

- **Remote tips only** — a stale local branch changes nothing, and no checkout is needed.
- **Prediction per commit and per item** — the same commit conflicts differently at different depths.
- **Runs in the queue owner's worktree with a fetch** — fetch touches only remote-tracking refs; this
  avoids a second clone per queue.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-commit-inbox-outbox-initial-discovery.md`)
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
