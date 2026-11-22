# Changeset: A work queue knows its lineage

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate, a proto addition, graph validation (cycles) and a read over n2's inbox, a graph-drawing web component, an optional external CLI probe
**Stack**: `#workqueue` 9/14 — branch `feature/workqueue/lineage-graph`, base `feature/workqueue/move-activities`
**PR**: [#1010](https://github.com/uppin/tddy-coder/pull/1010)

## Initial Discovery

[2026-11-22-lineage-graph-initial-discovery.md](./2026-11-22-lineage-graph-initial-discovery.md)

## Affected Packages

- **tddy-changeset**: `WorkItem.needs` (the stored dependency edges, one list per item), `DependencyEdge { from, to }`
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

## Stack note (temporary)

This branch skips n7 and n8: it is cut from n6's green (`472d1947b`), because n9's real dependency is n6.
In the line it sits on n8 and n7, whose implementations are not done; their contract commits are not
carried here. The branch is rebased onto n8 before n9's [W]. Remove this note then.

## Responsibility

- `DependencyEdge`, `WorkItem.needs` (the stored dependency edges, one list per item), `set_dependencies`, `detect_cycle`.
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

Published (pass 2, commit 2):

- `tddy_work_queue::lineage::{DependencyEdge, LineEndpoint (Item, Trunk, External), LineageError,
  line_endpoint_of_base, planned_line_parent, line_edges_through_inbox, set_dependencies,
  dependencies_of, detect_cycle}` — `line_parent_of` is `line_edges_through_inbox` (every item at once,
  PR base through the inbox, else the planned base)
- `tddy_tool_engine::work_queue_lineage_tools::{WORK_QUEUE_SET_DEPENDENCIES, WORK_QUEUE_GRAPH,
  WORK_QUEUE_LINEAGE_TOOL_NAMES, work_queue_lineage_tool_defs}`
- `tddy_github::gh_stack::gh_stack_membership` (optional probe)
- `tddy-workqueue-web`: `WorkQueueGraph` (shell, props-driven)
- The failing tests below. The `SetWorkQueueDependencies` / `GetWorkQueueGraph` RPCs are `/green`'s
  (the handler-default pattern of n6/n7); the graph component is tested on its props.

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

- [x] Stored dependency edges with cycle refusal
- [x] Line edges from PR bases; planned base for PR-less items; optional `gh stack` membership
- [x] Two tools + RPCs
- [x] Graph view
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

`parents: Vec<String>` (planned base + implied dependency); `Stack::topo_order` (Kahn); no line read.

### State B (Target)

```
line edge(item)  = endpoint(PR.base_branch)  — Item(k) if some item's branch, Trunk(b) if the trunk, External(b) otherwise
                 = planned(parents)           — when the item has no PR
dependency edges = WorkItem.needs          — stored, acyclic
```

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [x] Edge model + validation + cycle detection
- [x] Lineage read over the inbox; `gh stack` probe
- [x] Tools + RPCs
- [x] Graph view

## Testing Plan

Unit tests on cycle detection and endpoint resolution; integration through the daemon with an inbox
over a scripted GitHub (base-branch change without a queue write); a probe test with a stub `gh`
binary (with and without the extension); Cypress component spec for the graph.

## Acceptance Tests

`packages/tddy-work-queue/tests/lineage_unit.rs` (crate renamed `tddy-work-queue` by n3):
- [x] `a_pr_based_on_another_items_branch_has_a_line_edge_from_that_item`
- [x] `a_pr_based_on_the_trunk_has_a_trunk_endpoint`
- [x] `a_pr_based_on_a_branch_outside_the_queue_has_an_external_endpoint`
- [x] `an_item_without_a_pr_shows_its_planned_base`
- [x] `dependency_edges_are_stored_on_the_queue`
- [x] `a_dependency_set_closing_a_cycle_is_refused_naming_the_cycle`
- [x] `a_dependency_on_an_unknown_item_is_refused`

`packages/tddy-work-queue/tests/lineage_through_inbox_acceptance.rs` (real inbox, scripted GitHub):
- [x] `retargeting_a_pr_on_github_moves_its_line_edge_without_a_queue_write`

`packages/tddy-tool-engine/tests/work_queue_lineage_tools.rs`:
- [x] `the_lineage_tools_are_defined_and_setting_dependencies_takes_an_item_and_its_needs`

`packages/tddy-github/tests/gh_stack_membership.rs`:
- [x] `without_the_gh_stack_extension_there_is_no_membership`

`packages/tddy-workqueue-web/cypress/component/WorkQueueGraphAcceptance.cy.tsx`:
- [x] `line edges and dependency edges are drawn distinguishably`
- [x] `an external base is drawn as an endpoint outside the queue`

## Decisions & Trade-offs

- **Line edges are never stored** (operator decision 3: GitHub is the truth for them; lineage lives in
  tddy, not in a GitHub issue).
- **`gh stack` membership is optional**: no code reads it today and `gh stack view` reads local
  tracking only (premise 9, accepted); base refs are complete without it.
- **Dependency edges live on the item** (`WorkItem.needs`, omitted when empty), not in a queue-level
  list: a queue is a list of items, every existing literal `WorkQueue { version, nodes }` stays valid, and
  an item's deletion takes its edges with it. `dependencies_of` flattens them (`from` needs → `to` item),
  `detect_cycle` is exposed for n10's waves.
- **A line edge is read, never guessed**: an item whose PR the inbox cannot read keeps its planned base;
  the RPC and the tool say so (`unavailable_reason`) when the whole site is unreadable.
- **The `gh stack` probe reads `gh stack view --json` in the owner's checkout** — local tracking only —
  so membership is the stack of the checked-out branch; any miss is an empty list.
- **Both lineage tools are the owner's** (an addressed session reads status and a PR only, as before).
- **`parents` is not migrated into dependencies**: it carries a git meaning (where to cut and rebase).
  A planned item's base and its dependencies are different facts, set separately.

## Technical Debt & Production Readiness

- `WorkQueueGraphPanel` hides itself until the daemon answers, and when the client lacks the call.
- The status stream is not extended with line edges: the graph is read on mount and after each queue edit
  (`refreshKey`); a retarget on GitHub shows on the next read, not live. TODO(n10+): feed it from the
  inbox's PR events as the check counts are.

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-lineage-graph-initial-discovery.md`)
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
