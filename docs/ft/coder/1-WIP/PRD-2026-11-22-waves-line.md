# Waves, the derived line and readiness — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 10/14 — branch `feature/workqueue/waves-line`, base `feature/workqueue/lineage-graph`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)

## Summary

From a queue's dependency edges tddy computes:

- **Waves** — wave 1 is every item that needs no other item's behaviour; wave k is every item whose
  needs are all met by waves below it. An item with no needs is wave 1 wherever it sits.
- **The derived line** — the waves concatenated; inside a wave the blockers lead (most transitive
  dependents first; ties keep the queue's order). It is the order a branch chain *should* have.
- **Readiness** — for each item, the needs that actually bind it now: unmet needs in the wave
  immediately below. Deeper needs are not named, because they are met before those are.
- **Line check** — whether the current line (from PR bases, n9) matches the derived line, and if not,
  the first item out of place.

The queue view shows wave lanes, each item's readiness ("waits on n4, n6" / "ready"), and a line-check
banner. Agents get the same through `work_queue_waves` and in `work_queue_status`.

## Background

Today only `tddy-build` computes waves, for an unrelated graph; the queue has `topo_order` and
`display_order` but no notion of what can proceed in parallel or what an item is actually blocked on.
The planning rules this repo already uses for its own stacks (waves concatenated, blockers first,
never stepping back) are what the queue should compute.

## Proposed Changes

### What's Changing

- A pure `waves` module: `waves`, `wave_of`, `derive_line`, `transitive_dependents`, `readiness`,
  `line_check`.
- `work_queue_waves` tool + RPC; `work_queue_status` gains wave and readiness per item.
- Wave lanes, readiness text and the line-check banner in the Work queue tab.

### What's Staying the Same

- Stored data: nothing new is stored; everything is derived.
- `display_order` (the user's reading order) is independent of the derived line.

## Impact Analysis

### Technical Impact

A pure module with unit tests; one tool and RPC; a status field; web lanes.

### User Impact

The operator sees what can be worked in parallel, what each item waits on, and whether the branch
chain is in the right order.

## Implementation Plan

One node, `feature/workqueue/waves-line` (#workqueue 10/14).

## Acceptance Criteria

- [ ] Items with no needs are wave 1 regardless of position.
- [ ] Inside a wave, the item with the most transitive dependents leads; ties keep queue order.
- [ ] Readiness names only needs in the wave immediately below; a satisfied item reads "ready".
- [ ] A current line that steps back a wave is reported with the first item out of place.
- [ ] A dependency cycle reports the cycle instead of waves.
- [ ] The Work queue tab shows wave lanes and readiness.
