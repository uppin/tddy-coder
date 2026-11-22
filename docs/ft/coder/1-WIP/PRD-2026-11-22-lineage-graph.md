# A work queue knows its lineage — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 9/14 — branch `feature/workqueue/lineage-graph`, base `feature/workqueue/move-activities`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)

## Summary

A queue's structure has **two kinds of edge**, kept apart everywhere:

- **Line edges** — the branch chain on GitHub: an item's PR is based on another item's branch, on the
  trunk, or on a branch outside the queue. They are **read**, never stored: GitHub's PR base is the
  truth. Where the host's `gh` has the `gh stack` extension and the repository registers a stack, its
  membership is shown alongside; without it nothing is missing but that annotation.
- **Dependency edges** — what needs what *behaviour*: `a → b` means b needs a. They are **stored on the
  queue**, set by the agent or the user, and are the only edges later analysis (waves, readiness) reads.
  A cycle is refused, naming the path that closes it.

The queue view draws both — line edges solid along the branch chain, dependency edges dashed — and
`work_queue_graph` returns them to agents. An item without a PR has no line edge yet; its planned base
(`parents`) is shown as a planned line edge until a PR exists.

## Background

Today `parents` is the only edge and it means two things: the base a node's branch is cut from and
rebased onto, and what the plan says it depends on. A queue whose real structure branches (a DAG) but
lives on GitHub as a line cannot be described, and nothing reads the real base from GitHub.

## Proposed Changes

### What's Changing

- `WorkQueue.dependencies: Vec<{ from, to }>` with cycle refusal; tools
  `work_queue_set_dependencies { item_id, needs: [item_id] }` and `work_queue_graph`; matching RPCs.
- A lineage read: per item, its line parent endpoint (`Item`, `Trunk(branch)`, `External(branch)`)
  from its PR's base branch (inbox), else its planned base.
- Optional `gh stack` membership read on hosts where the extension is installed.
- The graph view in the Work queue tab (line and dependency edges, external endpoints).

### What's Staying the Same

- `parents` keeps its git meaning (the planned base used to cut, repoint and rebase).
- Waves and readiness are n10.

## Impact Analysis

### Technical Impact

A stored edge list with validation, a lineage read over the inbox, two tools, a graph component.

### User Impact

The operator sees the real branch chain and the real dependencies of a queue in one picture.

## Implementation Plan

One node, `feature/workqueue/lineage-graph` (#workqueue 9/14).

## Acceptance Criteria

- [ ] An item whose PR is based on another item's branch shows a line edge from that item.
- [ ] An item whose PR is based on the trunk, or on a branch outside the queue, shows that endpoint.
- [ ] Changing a PR's base on GitHub changes the line edge without any write to the queue.
- [ ] The agent sets dependency edges; a set that would close a cycle is refused naming the cycle.
- [ ] The graph view draws line edges and dependency edges distinguishably.
- [ ] Without the `gh stack` extension the graph is complete except for the membership annotation.
