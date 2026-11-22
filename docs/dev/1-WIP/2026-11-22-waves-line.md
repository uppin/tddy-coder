# Changeset: Waves, the derived line and readiness

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — a pure algorithm module with unit tests, one tool/RPC and a status field, web lanes; more than one package but no new I/O
**Stack**: `#workqueue` 10/14 — branch `feature/workqueue/waves-line`, base `feature/workqueue/lineage-graph`
**PR**: [#1011](https://github.com/uppin/tddy-coder/pull/1011)

## Initial Discovery

[2026-11-22-waves-line-initial-discovery.md](./2026-11-22-waves-line-initial-discovery.md)

## Affected Packages

- **tddy-work-queue**: `waves` module (`waves`, `wave_of`, `derive_line`, `transitive_dependents`, `readiness`, `line_check`)
- **tddy-service**: `work_queue.proto` `GetWorkQueueWaves`; `WorkItemStatus.{wave, binding_needs}`; closed lists
- **tddy-tool-engine**: `work_queue_waves`; status payload; closed lists
- **tddy-daemon-rpc**: handler; status assembly
- **tddy-tools**, **tddy-agent-launch** prompt: tool lists
- **tddy-workqueue-web**: `WaveLanes`, readiness text, `LineCheckBanner`

## Related Feature Documentation

- PRD: [PRD-2026-11-22-waves-line.md](../../ft/coder/1-WIP/PRD-2026-11-22-waves-line.md)

## Summary

Derived waves, blockers-first line, binding readiness and a line check over n9's edges.

## Background

Exploration 3 §4B.5: waves exist only in `tddy-build/src/graph.rs::waves` (not reusable across crates
as-is); the queue has `topo_order`, `display_order`, `assess::decide_next_action`.

## Responsibility

- The `waves` module and its five functions.
- `work_queue_waves`; wave and readiness in `work_queue_status`.
- `WaveLanes`, readiness text, `LineCheckBanner`.

## Boundaries

- No reordering of branches or PRs (fixing an out-of-order line is n14's propagation, or the operator's).
- No schedules (n13).
- Does not change how dependency edges or line edges are stored or read (n9).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n9` lineage-graph (#1010) | `WorkQueue.dependencies`, `detect_cycle`, `line_parent_of`, `LineEndpoint` | waves read dependency edges; the line check reads line parents; a cycle reports through `detect_cycle` | store or validate edges, or add a cycle check of its own |

## Draft PR contract

Published (pass 2, commit 2):

- `tddy_pr_stack::waves::{waves, derive_line, transitive_dependents, readiness, line_check, LineCheck}`
  — pure over `(items in queue order, &[DependencyEdge])`, so the tests use only n9's types, not its
  behaviour (`wave_of` is a lookup in `waves`' result)
- `tddy_tool_engine::work_queue_waves_tools::{WORK_QUEUE_WAVES, work_queue_waves_tool_def}`
- `tddy-prstack-web`: `WaveLanes` (shell, props-driven; it covers the line-check banner)
- The failing tests below. The `GetWorkQueueWaves` RPC and the status fields are `/green`'s.

## Green wave

**Wave:** 6 of 8
**Greenable independently:** yes once n9's types are in its base — the functions take the edges as input (pass 2), so no test needs n9's behaviour
**Concurrent with:** n11 commit-inbox-outbox, n12 leak-linearity
**Blocks:** n13 schedules (`schedule_wave` and the merge `order` condition)

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

## Green progress (paused at G:resume)

Done and pushed: the `waves` module (6 unit tests), the `work_queue_waves` tool definition (1 test),
`WaveLanes` (2 Cypress tests).

Remains, and stays in this node: the `GetWorkQueueWaves` RPC, `WorkItemStatus.{wave, binding_needs}`,
the proto and closed lists, the daemon-rpc handler, the tool-path wiring, and routing
`waves::cycle_among` through n9's `detect_cycle` (`TODO(n9)`). It waits for n9's green
(`lineage::dependencies_of` and `detect_cycle` are still stubs); resume then: rebase onto n9, finish.

## Scope

- [x] `waves` module
- [ ] Tool, RPC, status fields
- [x] Wave lanes, readiness, line-check banner
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

No wave computation for queues; `topo_order` only.

### State B (Target)

`waves(queue) -> Result<Vec<Vec<ItemId>>, Cycle>`; `derive_line(queue) -> Vec<ItemId>`;
`readiness(queue, item) -> Vec<ItemId>` (binding needs); `line_check(queue, current_line) -> InOrder | OutOfOrder { first }`.

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [x] `waves` module with unit tests
- [ ] Tool + RPC + status
- [x] Web lanes

## Testing Plan

Unit tests on the pure module (the example graphs from this repo's own planning rules make good
fixtures); one daemon integration test for the tool; a Cypress component spec for lanes.

## Acceptance Tests

`packages/tddy-pr-stack/tests/waves_unit.rs` (crate renamed `tddy-work-queue` by n3):
- [x] `an_item_with_no_needs_is_wave_one_wherever_it_sits`
- [x] `inside_a_wave_the_item_with_most_transitive_dependents_leads`
- [x] `ties_keep_the_queues_order`
- [x] `readiness_names_only_needs_in_the_wave_immediately_below`
- [x] `a_line_that_steps_back_a_wave_is_out_of_order_at_its_first_offender`
- [x] `a_cycle_reports_the_cycle_instead_of_waves`

`packages/tddy-tool-engine/tests/work_queue_waves_tools.rs`:
- [x] `work_queue_waves_is_defined_and_needs_no_argument`

Status carrying each item's wave and binding needs is proved in the daemon-rpc stack suite by `/green`.

`packages/tddy-workqueue-web/cypress/component/WorkQueueWavesAcceptance.cy.tsx`:
- [x] `items are drawn in wave lanes with their readiness`
- [x] `an out-of-order line shows the banner naming the first item out of place`

## Decisions & Trade-offs

- **A new module in `tddy-work-queue`**, not a reuse of `tddy-build`'s `kahn_waves`: different crate
  and graph type; the algorithm is small. Sharing it is a sweep-node candidate, not a prerequisite.
- **Readiness names binding needs only** — deeper needs are met before the binding ones resolve, so
  naming them misreports what an item is blocked on.
- **Nothing is stored** — the derived line changes whenever an edge does.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-waves-line-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code (partial, see Green progress)
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
