# Changeset: A work queue knows its lineage

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate, a proto addition, graph validation (cycles) and a read over n2's inbox, a graph-drawing web component, an optional external CLI probe
**Stack**: `#workqueue` 9/14 — branch `feature/workqueue/lineage-graph`, base `feature/workqueue/move-activities`

## Initial Discovery

[2026-11-22-lineage-graph-initial-discovery.md](./2026-11-22-lineage-graph-initial-discovery.md)

## Affected Packages

- **tddy-changeset**: `WorkQueue.dependencies`, `DependencyEdge { from, to }`
- **tddy-work-queue**: `lineage` module — `LineEndpoint::{Item, Trunk, External}`, `line_parent_of`, `detect_cycle` (names the closing path), `set_dependencies`
- **tddy-github**: `gh stack` membership probe through the host's `gh` (optional; absent extension = `None`)
- **tddy-service**: `work_queue.proto` `SetWorkQueueDependencies`, `GetWorkQueueGraph`; closed lists
- **tddy-tool-engine**: `work_queue_set_dependencies`, `work_queue_graph`; closed lists
- **tddy-daemon-rpc**: handlers; lineage read through the inbox (`pr(number)` base branch)
- **tddy-tools**, **tddy-agent-launch** prompt: tool lists
- **tddy-workqueue-web**: `WorkQueueGraph` (line and dependency edges, endpoints) in the Work queue tab

## Related Feature Documentation

- PRD: [PRD-2026-11-22-lineage-graph.md](../../ft/coder/1-WIP/PRD-2026-11-22-lineage-graph.md)

## Summary

Line edges read from PR bases (and optional `gh stack` membership); dependency edges stored with cycle
refusal; both drawn and returned to agents.

## Background

Exploration 3 §4A.4 (`parents` is the only edge; `topo_order` ignores dangling parents) and §3 (no
`gh stack` call anywhere). Exploration 2 §4A: PR base branches are in `PrDetail.base_branch`.

## Responsibility

- `DependencyEdge`, `WorkQueue.dependencies`, `set_dependencies`, `detect_cycle`.
- `LineEndpoint` and `line_parent_of(item)` — the single source of line edges for n10–n14.
- `work_queue_set_dependencies`, `work_queue_graph`, their RPCs.
- The `gh stack` membership probe.
- `WorkQueueGraph` (web).

## Boundaries

- No waves, derived line or readiness (n10); no commit analysis (n11); no leaks or linearity (n12).
- No GitHub write: a line edge is changed by retargeting the PR (n14's propagation), never here.
- `parents` keeps its git meaning; n9 does not migrate `parents` into dependencies.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` github-event-inbox (#1003) | `GithubInbox::pr(number)` with `base_branch` | line edges are read from it | read GitHub directly |
| `n5` session-work-queue (#1006) | session-keyed queue, owner writes | dependencies are owner writes | change access |
| `n6` pr-and-session-link (#1007) | `WorkItem.pr` (assigned number) | an item's PR for its base | assign or unassign PRs |
| `n8` move-activities (#1009) | `WorkQueueChange::ParentsChanged` | — (dependency edits are not moves) | publish activities |

## Draft PR contract

The first push of this PR, before its implementation: `DependencyEdge`, `LineEndpoint`,
`line_parent_of`, `detect_cycle` signatures, two tool definitions and RPCs, the graph component's test
ids, and the failing tests below.

## Green wave

**Wave:** 5 of 8
**Greenable independently:** no — line-edge tests read assigned PRs (n6) through the inbox (n2)
**Concurrent with:** none (wave 5 holds n9 alone)
**Blocks:** n10 waves-line (reads dependency edges), n11 commit-inbox-outbox and n12 leak-linearity (read line parents)

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

### ⚠ DURING — [2026-07-30-pr-stack-full-control-follow-ups.md](../todo/2026-07-30-pr-stack-full-control-follow-ups.md)

"`topo_order` ignores dangling parents": dependency edges are validated on write (an unknown item is
refused); `parents`' behaviour is unchanged.

### ⚠ DURING — `tddy-changeset/src/changeset/stack.rs` at 463 production lines

The new field and validation go in a new module, not into the item file.

## Scope

- [ ] Stored dependency edges with cycle refusal
- [ ] Line edges from PR bases; planned base for PR-less items; optional `gh stack` membership
- [ ] Two tools + RPCs
- [ ] Graph view
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

`parents: Vec<String>` (planned base + implied dependency); `Stack::topo_order` (Kahn); no line read.

### State B (Target)

```
line edge(item)  = endpoint(PR.base_branch)  — Item(k) if some item's branch, Trunk(b) if the trunk, External(b) otherwise
                 = planned(parents)           — when the item has no PR
dependency edges = WorkQueue.dependencies     — stored, acyclic
```

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [ ] Edge model + validation + cycle detection
- [ ] Lineage read over the inbox; `gh stack` probe
- [ ] Tools + RPCs
- [ ] Graph view

## Testing Plan

Unit tests on cycle detection and endpoint resolution; integration through the daemon with an inbox
over a scripted GitHub (base-branch change without a queue write); a probe test with a stub `gh`
binary (with and without the extension); Cypress component spec for the graph.

## Acceptance Tests

`packages/tddy-work-queue/tests/lineage_unit.rs`:
- [ ] `a_pr_based_on_another_items_branch_has_a_line_edge_from_that_item`
- [ ] `a_pr_based_on_the_trunk_has_a_trunk_endpoint`
- [ ] `a_pr_based_on_a_branch_outside_the_queue_has_an_external_endpoint`
- [ ] `an_item_without_a_pr_shows_its_planned_base`
- [ ] `a_dependency_set_closing_a_cycle_is_refused_naming_the_cycle`

`packages/tddy-daemon-rpc/tests/work_queue_lineage_acceptance.rs`:
- [ ] `retargeting_a_pr_on_github_moves_its_line_edge_without_a_queue_write`
- [ ] `the_graph_carries_both_edge_kinds`
- [ ] `without_the_gh_stack_extension_the_graph_has_no_membership_and_nothing_else_missing`

`packages/tddy-workqueue-web/cypress/component/WorkQueueGraphAcceptance.cy.tsx`:
- [ ] `line edges and dependency edges are drawn distinguishably`
- [ ] `an external base is drawn as an endpoint outside the queue`

## Decisions & Trade-offs

- **Line edges are never stored** (operator decision 3: GitHub is the truth for them; lineage lives in
  tddy, not in a GitHub issue).
- **`gh stack` membership is optional**: no code reads it today and `gh stack view` reads local
  tracking only (premise 9, accepted); base refs are complete without it.
- **`parents` is not migrated into dependencies**: it carries a git meaning (where to cut and rebase).
  A planned item's base and its dependencies are different facts, set separately.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-lineage-graph-initial-discovery.md`)
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
