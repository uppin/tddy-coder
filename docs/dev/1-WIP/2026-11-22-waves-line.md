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

- `tddy_work_queue::waves::{waves, derive_line, transitive_dependents, readiness, line_check, LineCheck}`
  — pure over `(items in queue order, &[DependencyEdge])`, so the tests use only n9's types, not its
  behaviour (`wave_of` is a lookup in `waves`' result)
- `tddy_tool_engine::work_queue_waves_tools::{WORK_QUEUE_WAVES, work_queue_waves_tool_def}`
- `tddy-workqueue-web`: `WaveLanes` (shell, props-driven; it covers the line-check banner)
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

## Green progress

Done: the `waves` module (6 unit tests, with `queue_waves` for what the RPC, the tool and the status share;
a cycle is reported through n9's `lineage::detect_cycle`), the `work_queue_waves` tool (definition, op,
answer; in the closed tool lists), `GetWorkQueueWaves` and `WorkItemStatus.{wave, binding_needs}` (proto,
handler, status stream, regenerated bindings), `WaveLanes` (2 Cypress tests). `work_queue_status` carries
each node's `wave` and `binding_needs` too. Waves and the line stay structural; readiness treats a merged item as a met need (stored `pr_status`/`internal_status` merged, plus the live PR state in the status stream) — added in validation, with 3 unit tests and one `GetWorkQueueWaves` acceptance test.

## Scope

- [x] `waves` module
- [x] Tool, RPC, status fields
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
- [x] Tool + RPC + status
- [x] Web lanes

## Testing Plan

Unit tests on the pure module (the example graphs from this repo's own planning rules make good
fixtures); one daemon integration test for the tool; a Cypress component spec for lanes.

## Acceptance Tests

`packages/tddy-work-queue/tests/waves_unit.rs`:
- [x] `an_item_with_no_needs_is_wave_one_wherever_it_sits`
- [x] `inside_a_wave_the_item_with_most_transitive_dependents_leads`
- [x] `ties_keep_the_queues_order`
- [x] `readiness_names_only_needs_in_the_wave_immediately_below`
- [x] `a_line_that_steps_back_a_wave_is_out_of_order_at_its_first_offender`
- [x] `a_cycle_reports_the_cycle_instead_of_waves`

`packages/tddy-tool-engine/tests/work_queue_waves_tools.rs`:
- [x] `work_queue_waves_is_defined_and_needs_no_argument`

Status carrying each item's wave and binding needs is proved in the daemon-rpc stack suite
(`stack_plan_tools_acceptance`: `status_carries_each_items_wave_and_binding_needs_and_the_waves_tool_answers_them`),
and `GetWorkQueueWaves` in `work_queue_lineage_rpc_acceptance`.

`packages/tddy-workqueue-web/cypress/component/WorkQueueWavesAcceptance.cy.tsx`:
- [x] `items are drawn in wave lanes with their readiness`
- [x] `an out-of-order line shows the banner naming the first item out of place`

## Decisions & Trade-offs

- **A new module in `tddy-work-queue`**, not a reuse of `tddy-build`'s `kahn_waves`: different crate
  and graph type; the algorithm is small. Sharing it is a sweep-node candidate, not a prerequisite.
- **Readiness names binding needs only** — deeper needs are met before the binding ones resolve, so
  naming them misreports what an item is blocked on.
- **A merged need is met.** Readiness is not purely structural: `queue_waves` takes the landed items (`merged_items`, plus the inbox's live merged PRs in the status) so a merged item no longer shows as 'waits on'. The item keeps its wave.
- **Nothing is stored** — the derived line changes whenever an edge does.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(none outstanding)_

## Validation Results

- Stack gate: rebased onto `lineage-graph` (dd9e76f35); `origin/<base>..HEAD` holds only n10's commits.
- Boundary: Responsibility delivered; no n9 symbol re-implemented (edges and cycle detection consumed from `lineage`); no n13 schedules.
- Gap found and fixed: readiness ignored merged items → `merged_items` + `done` argument, tests added.
- Scoped gates: `cargo check --all-targets` and `./dev cargo clippy -D warnings` clean on tddy-work-queue, tddy-daemon-rpc (+check on tool-engine, tools, service, agent-launch); waves_unit 9, lineage rpc 6, stack_plan 13, catalogue/access/dispatch green.
- Risk: Critical 0, Warning 0, Info 1 (status stream reads each item's PR sequentially, as before).

## TODO

- [x] Record initial discovery (`2026-11-22-waves-line-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [x] Validate changes (/validate-changes)
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
