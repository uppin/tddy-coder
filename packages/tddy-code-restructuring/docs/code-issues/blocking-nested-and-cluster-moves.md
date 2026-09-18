# blocking: `move_module_to_crate` cannot move the shapes this workspace has

**Location:** `packages/tddy-code-restructuring/src/crate_move.rs:773` `source_crate_of`, `:573` `refuse_a_dependency_cycle`, `runner.rs:804` `StatePaths::under`
**Category:** blocking
**Detected:** 2026-09-10 by `#unbundle` nodes 1–3 (recorded in `docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md` and `2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md`); re-confirmed 2026-09-15
**Metrics:** moved **13 of 21** modules on `#unbundle` node 1, **0 of ~24** on node 2, **0 of 4** entangled on node 3 · 4 distinct refusals
**Restructure:** no — this **is** the restructuring tool
**Status:** Open — **partially fixed**; claimed by #490 for the remainder
**Fixed so far:** #488 (**merged** 2026-09-18) — nested anchors, defining-crate attribution for facades, `check`/`apply` precondition parity
**Claimed by:** #490 — `#carve` 3/10 `restructure-clusters` (multi-module clusters, plan-scoped journal) · draft · `feature/carve/restructure-clusters`
**Lands after:** #489 (#488 has merged)

## Measurement history

| Run | Modules moved | Refusals standing | Note |
|---|---|---|---|
| 2026-09-10 | 13 / 21, then 0 / ~24, then 0 / 4 | 4 | measured across `#unbundle` nodes 1–3 |
| 2026-09-15 | — | 3 | `--indexing-budget` found **already fixed**; the other three still in the code |
| 2026-09-18 | — | **2** | #488 merged: nested anchors and facade attribution closed. **One-module-at-a-time and the repo-scoped journal remain** |

## What the tool found

| Refusal | Where | Effect |
|---|---|---|
| ~~Flat modules only~~ | `source_crate_of` | **CLOSED by #488** — a nested anchor now resolves its parent by walking `<crate>/src/<parent>.rs` then `<parent>/mod.rs` |
| ~~Origin facades read as cycles~~ | `refuse_a_dependency_cycle` | **CLOSED by #488** — a path is now attributed to the crate that *defines* the item |
| **One module at a time** | `struct Move` is singular; `repointed_header` takes the origin as a scalar | a mutually-referencing set cannot move: the tree does not compile between the first op and the last |
| **Repo-scoped journal** | `StatePaths::under(root)` → one `.restructure/` per repository | a *completed* plan blocks the next, and `--resume` would resume the wrong one. Every multi-phase node pays it |

And the worse half of two of them: **`restructure check` reported `no findings`** on plans that
`apply` then rejected outright — **also closed by #488**, which gave `check` the same preconditions
`apply` runs.

## Why it matters here

This tool is how the repo is supposed to perform mechanical moves — `code-restructuring` mandates
it, and CLAUDE.md's judgment boundaries mean hand-moving code is the fallback, not the plan. While
these stand, every cross-crate extraction degrades to `git mv` plus hand-edited imports, which is
what actually happened across `#unbundle`.

## What would close it — narrowed 2026-09-18

**Two of the four refusals are gone** (#488, merged). What remains, and what #490 owns:

1. **One module at a time.** `struct Move` is singular and `repointed_header` takes the origin extern
   name as a scalar, so a mutually-referencing set still cannot move atomically.
2. **Repo-scoped run state.** `StatePaths::under(root)` still keys `.restructure/` by repository, so
   a completed plan blocks the next one.

This record stays **open** until both land. It is deliberately not resolved: #488 fixed half, and a
partial fix written up as resolved would drop the remainder out of every open-items query.

**`--indexing-budget` is already fixed** and its backlog section is stale — `request_timeout` reads
the budget, `settle_budget_for` scales the per-operation wait, and `map_lsp_error(Timeout)` maps to
a retryable state. #488 verifies and closes it rather than re-implementing it.

## If you are about to change this code

#488 has **merged**, so `crate_move.rs` is no longer the most contended file in the repo — but #490
rewrites more of it than any other node (its Phase A carves the file before its Phase B edits it).
Coordinate before planning concurrent work there.

If you merely *use* the tool, none of this affects you: #490 adds capability and changes no existing
operation's behaviour, which its own tests pin.

## Verified by hand

2026-09-18: #488 merged. Re-checked: nested anchors and facade attribution are closed; the singular
`Move` model and `StatePaths::under` remain. Narrowed *What would close it* to those two rather than
resolving the record.

2026-09-15: confirmed all three code-level refusals still present at the cited lines, and confirmed
the fourth (`--indexing-budget`) is fixed — the backlog entry predates the fix, whose last edit was
`#unbundle 3/10` while the fix landed later. Both source todos remain the authoritative narrative;
this record is the standing measurement.
