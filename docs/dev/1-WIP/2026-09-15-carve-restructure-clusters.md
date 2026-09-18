# Changeset: carve-restructure-clusters

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Feature (tooling capability)
**Stack**: `#carve` 3/10

> **Numbering note.** These documents were written when the stack was planned as **9** nodes and say
> `3/9` throughout; the stack is now **10**, which is what the PR titles and the merged
> `#carve 1/10` / `#carve 2/10` subjects on `master` say. `N` is the live number — renumbering the
> prose is `/pr-wrap`'s title pass, not green's.
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
- [x] Implement production code making tests pass (`/green`) — 380 passed / 0 failed across 14 targets
- [x] Re-file the two cosmetic restructure defects as their own todo entry — [`2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md`](../todo/2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md)
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Validation Results — 2026-09-18 `/pr-wrap` step 1

**Risk summary: 1 critical, 2 warnings.** Every stack-boundary check is clean; the critical finding
is a **reachability gap**, not a defect in the code that was written.

### 🔴 CRITICAL — FR1/FR2 are unreachable through the tool, so AC1/AC2 fail live

`resolve_cluster` is correct and well tested, but **nothing can invoke it with more than one member**:

- `RefactorOp` has no field for a co-moving set, and `src/plan.rs` is **untouched** by this PR, so a
  `plan.jsonl` cannot declare a cluster.
- The apply loop (`runner/entry_points.rs:149`) is `for (index, op) in plan.ops.iter()` — one op at a
  time. Each resolves via `resolve()`, which builds a **one-member** cluster (`travelling_alone`).
- So the only production call site passes a set of one, and `co_moving()` never contains a sibling.

**Measured live**, on a throwaway git workspace with `spawner` ⇄ `spawn_worker` mutually referencing
and *both* named in a 2-op plan:

    $ tddy-tools restructure check plan.jsonl
    no findings                                    # correct: nothing is stranded, both are in the plan

    $ tddy-tools restructure apply plan.jsonl
    Error: plan is malformed: `crates/origin/src/spawner.rs` still names `origin`
    (origin::spawn_worker::Worker), so the destination would depend on the crate it left …

**Moved: 0 of 2.** That is State A verbatim — the `#unbundle` node 3 failure this PR exists to fix,
whose headline metric was "0 of 4". Nothing was written (the refusal fires before any edit), so the
tool is safe; it simply cannot do the thing.

Consequences:

| Claim | Status |
|---|---|
| FR1 "`move_module_to_crate` accepts a set … resolved and applied atomically" | ❌ not through the tool; library-only |
| FR2 "the header pass is told the co-moving set" | ❌ the set is never populated from a plan |
| AC1 "a set of N mutually-referencing modules moves in one operation" | ❌ fails live |
| AC2 "a `crate::<sibling>` path … re-pointed at the **destination**" | ❌ fails live |
| AC4 "does not add the destination as a dependency of itself" | ✅ the refusal holds, nothing written |
| FR3 / AC5 / AC6 plan-scoped state | ✅ delivered and reachable |
| FR4 / AC7 `check` reports a partial cluster | ✅ delivered and reachable |
| AC8 single-module unchanged | ✅ three live suites green |
| PRD "What it unblocks" (nodes 6/9, 7/9, 9/9) | ❌ still blocked — they drive the tool with `plan.jsonl` |

**What is missing** is the wiring, and it carries a design decision this changeset never took: the
journal and ledger are **per operation**, while a cluster is **one edit spanning N ops**. Either a
plan gains vocabulary for a set (one op, one journal entry) or the apply loop groups ops and the
journal learns to span them.

### ⚠️ WARNING — FR3 has one consumer still on the repo-scoped layout

`packages/tddy-index-daemon/src/apply.rs:45` keeps `StatePaths::under(root)`. One line, but it
invalidates a per-root-queue rationale stated in four doc comments and in that package's committed
docs. Filed as `packages/tddy-index-daemon/docs/code-issues/stale-repo-scoped-restructure-state-apply.md`.

### ⚠️ WARNING — two files are close to the 500-production-line gate

`crate_move/cluster.rs` at 474 and `runner/entry_points.rs` at **493** (grew 482 → 493 here, 7 lines
of headroom). Neither breaches the gate; both will on the next change.

### Clean

| Check | Result |
|---|---|
| Rebase + leak check (`origin/master..HEAD` is this PR only) | ✅ 4 commits, 21 files, all this PR's |
| `## Dependencies` not implemented here | ✅ every node-1 function is byte-identical modulo module-path requalification and `pub(crate)` |
| `## Boundaries` respected | ✅ no crate carved; both cosmetic-defect sites logic-identical (correctly **not** fixed) |
| No dependent's behaviour, no unplanned deletions | ✅ |
| Builds, clippy `--all-targets -D warnings`, `fmt --all --check` | ✅ both packages |
| Tests | ✅ 380 passed / 0 failed, 14 targets |
| Test quality | ✅ 13 new tests, 3 Given/When/Then each, named builders; `tests/cluster_move.rs` untouched |
| Debug output / `#[allow]` / new TODO-FIXME | ✅ none |
| File length gate | ✅ none ≥ 500; `crate_move.rs` 1,340 → 320 |

## Verification

**The inherited red is gone.** That baseline (297 passed / 10 failed) was recorded while `#carve`
1/9 was un-greened below this branch. 1/9 merged as [#488](https://github.com/uppin/tddy-coder/pull/488)
and 2/9 as [#489](https://github.com/uppin/tddy-coder/pull/489), so this branch was rebased onto
`master` with `--onto` and its range is now this node's commits only.

**Measure this crate with `--no-fail-fast`.** `./test` does not pass it, and `tests/cluster_move.rs`
is only the 3rd of 13 suites alphabetically, so while it was red a plain `./test` aborted there and
**silently skipped nine suites**. The honest baseline was **362 passed / 4 failed** across 14 targets;
green is **380 / 0**.

```bash
cargo test -p tddy-code-restructuring --all-targets --no-fail-fast
cargo clippy -p tddy-code-restructuring -p tddy-index-daemon --all-targets -- -D warnings
cargo fmt --all --check
```

Plus a **live** cluster apply against this workspace. The backlog records that the refusals are
unit-tested only, and that this is exactly what let both defects reach a real stack — so AC1 and AC5
are proven against a real rust-analyzer, not a double.

⚠️ **Outstanding for `/validate-changes`, and deliberately not done in green:**

- **No live cluster apply.** A live run would move files in this workspace's own crates, which
  `## Boundaries` forbid. AC1–AC4 are covered by unit tests over real temp workspaces, and AC8 by
  three real-rust-analyzer acceptance suites (`move_module_to_crate_acceptance` 2,
  `nested_module_move_acceptance` 4, `facade_cycle_acceptance` 3). A rehearsal on a throwaway tree is
  still owed.
- **AC5 end-to-end is argued, not tested.** Two plans applied back to back is reasoned from the
  directory being empty, not pinned by a test; such a test needs a git worktree and a live server.
- **FR3 has one consumer still opting out.** `packages/tddy-index-daemon/src/apply.rs:45` keeps
  `StatePaths::under(root)`. One line, but it invalidates the per-root queue rationale in four doc
  comments and in that package's committed docs, which is changeset work in another package — filed
  as [`stale-repo-scoped-restructure-state-apply.md`](../../../packages/tddy-index-daemon/docs/code-issues/stale-repo-scoped-restructure-state-apply.md).
