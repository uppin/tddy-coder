# Changeset: carve-restructure-clusters

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Feature (tooling capability)
**Stack**: `#carve` 3/9
**PR**: [#490](https://github.com/uppin/tddy-coder/pull/490)

PRD: [`2026-09-15-carve-restructure-clusters-prd.md`](./2026-09-15-carve-restructure-clusters-prd.md)

## Initial Discovery

[`2026-09-15-carve-restructure-clusters-initial-discovery.md`](./2026-09-15-carve-restructure-clusters-initial-discovery.md)

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md) —
  `move_module_to_crate` takes a set of co-moving modules; `.restructure/` state is plan-scoped.
- **`docs/ft/coder`**: [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) —
  § Known limitations loses "one module at a time".

## Responsibility

- `move_module_to_crate` resolves and applies a **set** of modules atomically — the whole cluster or
  none of it.
- The header pass is told the co-moving set: a `crate::<sibling>` path where the sibling is in the
  set is re-pointed at the **destination**; one staying behind is re-pointed at the origin as today.
- `.restructure/{journal.jsonl,ledger.json}` are keyed by the plan, not by the repository.
- `check` reports a cluster that cannot move — a module whose siblings reference it and are not in
  the set — statically, without spawning rust-analyzer.

## Boundaries

- Does **not** carve any crate. No `tddy-core`, `tddy-session-lifecycle` or `tddy-workflow-recipes`
  file moves.
- Does **not** re-fix the nested-module or facade-cycle refusals, or `check`/`apply` parity itself —
  `#carve` 1/9 owns all three, and this node **builds on** them.
- Does **not** fix the cosmetic defects (one `pub use <crate>::*;` per operation rather than per
  destination; `pub mod` lines appended out of order).
- Does **not** change single-module behaviour, which the existing tests pin.
- Does **not** decide which clusters the later nodes move — that is each node's own plan.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | `refuse_a_dependency_cycle` attributing a path to the crate that **defines** the item; `check` running `apply`'s preconditions; nested anchors accepted | FR2/FR4 extend the defining-crate attribution to treat a co-moving sibling as destination-local; FR4 adds one more precondition to the pass 1/9 created | re-implement the attribution, the nested resolver, or the `check` pass — all three are 1/9's surface |
| `2/9` recipe-parsers | nothing this node consumes | — | touch `tddy-workflow-recipes` |

## Draft PR contract

Published first:

**Published** (commit 2), all `pub` and re-exported from `lib.rs`:

1. `MovingCluster { members, destination, reexport }` with `co_moving()` — the set-carrying shape.
   `co_moving()` is **real**, not a `todo!()`: it is the distinction the single-module model cannot
   express, and every other criterion rests on it.
2. `resolve_cluster(engine, workspace, cluster)` — one edit, applied all or not at all.
3. `siblings_left_behind(workspace, ops)` — the `check` finding for a partial cluster move.
4. `state_directory_for_plan(root, plan)` in `runner.rs` — run state keyed by the plan.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 2 of 4
**Greenable independently:** **no** — FR2 and FR4 extend the defining-crate attribution and the
`check` precondition pass that `#carve` 1/9 creates. Both must exist as behaviour, not just as
signatures, before this node's tests can pass
**Concurrent with:** `#carve` 4/9 `core-foundations`, 5/9 `git-plumbing`
**Blocks:** 6/9 `session-store`, 7/9 `telegram`, 9/9 `pr-stack-crate`

Real dependency edges, as opposed to the branch line:

    n1 → n3, n4, n5, n6, n9      n3 → n6, n7, n9      n4 → n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md](../todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md) | ✅ **RESOLVED HERE** | Part 1 (entangled cluster) is this node's FR1/FR2/FR4. Part 2 (`--indexing-budget`) was found already fixed and edited out by `#carve` 1/9. **This node's wrap deletes the file.** |
| [2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) | ✅ **RESOLVED HERE** | Its nested-module and facade-cycle halves were fixed by `#carve` 1/9, which edited the entry down; this node fixes the remaining repo-scoped-journal defect. **This node's wrap deletes the file.** The two cosmetic items are re-filed as a new entry rather than carried. |
| [2026-09-09-restructure-defects-from-the-connection-service-split.md](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md) | ℹ **Answered** | D6–D9 recorded fixed; no work here. |

**This node claims both entries** because it is the lowest node that fixes each one *completely* —
`#carve` 1/9 edited both down but closed neither. Before deleting either file, re-file the two
cosmetic defects as their own `docs/dev/todo/` entry; they are unfixed and must not leave with the
file that recorded them.

## State A → State B

### State A

- `crate_move.rs:341` — `struct Move` holds one `source`, one `module`, one `origin`, one
  `destination`. Single-module by construction.
- `crate_move.rs:647` — `repointed_header(text, origin_extern)` re-points **every** `crate::` path at
  the origin's extern name. No notion of a sibling that is also moving.
- `runner.rs:804` — `StatePaths::under(root)` → `<root>/.restructure/…`, one per repository.
- `runner.rs:779` — `open_run` returns `JournalExists` when a non-empty journal exists and neither
  `--resume` nor `--from` was given.
- Measured: `#unbundle` node 3 moved **0 of 4** entangled modules; `check` reported `no findings` on
  the four-op plan that `apply` then rejected.

### State B

- A cluster moves atomically; the tree never sits in a non-compiling intermediate state.
- A co-moving sibling reference is destination-local; the operation cannot author a self-dependency.
- Two plans run back to back with no hand-archiving.
- `check` names a cluster it cannot move.

## Implementation phases

| Phase | Kind | Work |
|---|---|---|
| **A** | mechanical | `extract_module --to_file` splitting `crate_move.rs` (1,500+ lines) along the seam its own structure names — `Move`/`Destination` resolution, the header pass, the refusals, the manifest edits |
| **B** | manual | Widen `Move` to a cluster; thread the co-moving set into `repointed_header`; key `StatePaths` by plan; the new `check` precondition |
| **C** | mechanical | `extract_module --to_file` for whatever Phase B leaves oversized — measured, not assumed |
| **D** | manual | `README.md`, `docs/ft/coder/rust-code-restructuring.md` § Known limitations, re-file the two cosmetic defects, delete both claimed backlog entries at wrap |

Phase A runs **before** Phase B deliberately: `crate_move.rs` is the file this node rewrites most,
and carving it first means the manual work lands in files small enough to review.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (cluster surface + failing tests)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `tests/cluster_move.rs` — 4 failing on `siblings_left_behind` and `state_directory_for_plan`;
    `names_the_modules_travelling_together` passes, pinning the data shape the rest builds on.
- [x] Failing unit/integration tests — covered by the same suite; neither decision needs a server, which is the point
- [ ] Implement production code making tests pass (`/green`)
- [ ] Re-file the two cosmetic restructure defects as their own todo entry
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

**Inherited red from `#carve` 1/9:** this branch's baseline is **297 passed, 10 failed** — node 1's
own failing tests, which it has not been greened on yet. They are its red, not this node's.

```bash
./test -p tddy-code-restructuring
cargo clippy -p tddy-code-restructuring -- -D warnings
cargo fmt --all --check
```

Plus a **live** cluster apply against this workspace. The backlog records that the refusals are
unit-tested only, and that this is exactly what let both defects reach a real stack — so AC1 and AC5
are proven against a real rust-analyzer, not a double.
