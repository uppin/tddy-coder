# Changeset: carve-presenter-split

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 8/9

PRD: [`2026-09-15-carve-presenter-split-prd.md`](./2026-09-15-carve-presenter-split-prd.md)

## Initial Discovery

[`2026-09-15-carve-presenter-split-initial-discovery.md`](./2026-09-15-carve-presenter-split-initial-discovery.md)

## Affected Packages

- **`tddy-core`**: [README.md](../../../packages/tddy-core/README.md) — `presenter_impl.rs` becomes a
  directory of six `impl Presenter` blocks.

## Responsibility

- Partition the single 46-method `impl Presenter` into six blocks along `#carve` 4/9's sub-struct
  boundaries, by hand, without touching a method body.
- Move each whole block into its own module with `extract_module --to_file`.
- Leave the struct, its three state accessors and the module declarations in `presenter_impl.rs`.

## Boundaries

- Does **not** change the sub-structs — `#carve` 4/9 owns them. A method group that straddles two is
  **reported**, not fixed by moving a field.
- Does **not** touch `presenter/workflow_runner.rs` (1,015 prod lines) or the existing
  `presenter_impl/agent_activity_stamping.rs`.
- Does **not** change any signature, any visibility, or any behaviour.
- Does **not** leave `tddy-core`. No cross-crate move, so `move_module_to_crate` is not on its path.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `4/9` core-foundations | `WorkflowRun`, `PendingQuestions`, `ActivityRecorder`, `ViewChannels`, `BackendSelection` as owned sub-structs of `Presenter` | the six-way method partition **is** those boundaries; without the sub-structs there is nothing to partition along | add, rename, re-scope or move a field between sub-structs |
| `1/9`, `3/9` | restructure tooling fixes | **not consumed** — every operation here is in-crate `extract_module`, which already works | touch `tddy-code-restructuring` |
| `5/9`, `6/9`, `7/9` | crate extractions | **not consumed** | touch any of them |

## Draft PR contract

Published first:

1. The six module files, each holding one `impl Presenter` block, produced by the partition and the
   extraction.
2. Failing assertions pinning AC2, AC3 and AC6.

No new API surface — every method keeps its name, signature and visibility. What lands first is the
shape.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** **no** — the partition is *defined* by `#carve` 4/9's sub-structs. They
must exist as real fields, not just as declared types, for AC4 to mean anything
**Concurrent with:** `#carve` 6/9 `session-store`, 7/9 `telegram`
**Blocks:** nothing

Real dependency edges, as refined by `#carve` 6/9's discovery:

    n1 → n3, n4, n5, n6, n9      n3 → n7, n9      n4 → n6, n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-09-12-the-two-new-service-rs-files-are-over-budget.md](../todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md) | ℹ **Answered** | Records the file-budget convention and the reason two files were left whole — *"a split for line count alone cuts cohesive units and puts churn on top of a move"*. This node's seams are **state** boundaries, not size boundaries, which is the distinction that entry draws. Not claimed. |
| [2026-09-09-macro-expansion-as-a-restructure-operation.md](../todo/2026-09-09-macro-expansion-as-a-restructure-operation.md) | — Unrelated | No macro in this file's path. |

## State A → State B

### State A

- `presenter_impl.rs` — 1,788 production lines. One struct (37 fields before `#carve` 4/9, seven
  after) and **one `impl Presenter` block from line 152 holding 46 methods**.
- The methods call each other freely: `poll_workflow` → `broadcast`, `log_activity`;
  `handle_intent` → `collect_answers`.
- `presenter/presenter_impl/agent_activity_stamping.rs` (299 lines) already exists.

### State B

- Six modules, one `impl Presenter` block each, partitioned along the sub-struct boundaries.
- `presenter_impl.rs` under 250 production lines.

## Implementation phases

The constraint that shapes every phase: **`extract_module` refuses to lift one member out of an
`impl` its siblings call**, and no ordering fixes it — an `impl` body cannot hold a `mod`. The plan
schema's remedy is to *grow the seam to carry the whole `impl`*.

| Phase | Kind | Work |
|---|---|---|
| **A** | manual | Close and reopen `impl Presenter { … }` six times **in place**, partitioning the 46 methods. No method body is edited — only the block boundaries between them. This is the prep that turns a refused operation into a supported one |
| **B** | mechanical | `extract_module --to_file` × 6, each anchored on a whole `impl` block. **Moving a whole `impl` is free of caller churn** — a method is reached through its type — so no call site anywhere changes |
| **C** | manual | Any field visibility the split requires: a sub-struct field reached from a sibling module needs `pub(super)` within the presenter tree. `#carve` 4/9's AC7 caps it there — nothing becomes `pub` outside the module |
| **D** | manual | `README.md` module table; report any method group that straddled two sub-structs |

Phase A is small and mechanical in character but has no operation: partitioning one `impl` into six
is a text edit at five points, and there is no assist for "split this impl".

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [ ] Publish the draft-PR contract (six modules + failing budget/verify assertions)
- [ ] Failing acceptance tests — **USER REVIEW**
- [ ] Failing unit/integration tests
- [ ] Implement production code making tests pass (`/green`)
- [ ] Report any method group straddling two sub-structs
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-core
cargo clippy -p tddy-core -- -D warnings
cargo fmt --all --check
tddy-tools restructure verify --against HEAD
```

`verify --against` is the load-bearing check here: a partition that accidentally edits a method body
is exactly what it catches, statement by statement.
