# blocking: `move_module_to_crate` cannot move the shapes this workspace has

**Location:** `packages/tddy-index-daemon/src/apply.rs:45` `StatePaths::under` (the last repo-scoped caller)
**Category:** blocking
**Detected:** 2026-09-10 by `#unbundle` nodes 1–3 (recorded in `docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md` and `2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md`); re-confirmed 2026-09-15
**Metrics:** moved **13 of 21** modules on `#unbundle` node 1, **0 of ~24** on node 2, **0 of 4** entangled on node 3 · 4 distinct refusals
**Restructure:** no — this **is** the restructuring tool
**Status:** Open — **narrowed to one consumer**; the tool itself is fixed
**Fixed so far:** #488 (**merged** 2026-09-18) — nested anchors, defining-crate attribution for facades, `check`/`apply` precondition parity · #490 (green 2026-09-18) — multi-module clusters, plan-scoped run state on both CLI entry points
**Claimed by:** nobody — the remainder is one line in a *consumer*, not in this tool
**Lands after:** #489 (#488 has merged)

## Measurement history

| Run | Modules moved | Refusals standing | Note |
|---|---|---|---|
| 2026-09-10 | 13 / 21, then 0 / ~24, then 0 / 4 | 4 | measured across `#unbundle` nodes 1–3 |
| 2026-09-15 | — | 3 | `--indexing-budget` found **already fixed**; the other three still in the code |
| 2026-09-18 | — | **2** | #488 merged: nested anchors and facade attribution closed. **One-module-at-a-time and the repo-scoped journal remain** |
| 2026-09-18 (green #490) | **4 / 4** entangled, in one operation | **0** | Both remaining refusals closed. `resolve` is now `resolve_cluster` over a one-member set, so the single-module path *is* the cluster path — 380 passed / 0 failed across 14 targets, including three real-rust-analyzer acceptance suites |

## What the tool found

| Refusal | Where | Effect |
|---|---|---|
| ~~Flat modules only~~ | `source_crate_of` | **CLOSED by #488** — a nested anchor now resolves its parent by walking `<crate>/src/<parent>.rs` then `<parent>/mod.rs` |
| ~~Origin facades read as cycles~~ | `refuse_a_dependency_cycle` | **CLOSED by #488** — a path is now attributed to the crate that *defines* the item |
| ~~One module at a time~~ | `struct Move` was singular; `repointed_header` took the origin as a scalar | **CLOSED by #490** — `MovingCluster` + `resolve_cluster` resolve a set into one `WorkspaceEdit`, and the header pass re-points a co-moving sibling at the destination |
| ~~Repo-scoped journal~~ | `StatePaths::under(root)` → one `.restructure/` per repository | **CLOSED by #490** for `restructure apply` and `status`, which now use `StatePaths::for_plan`. One consumer still opts out — see below |

And the worse half of two of them: **`restructure check` reported `no findings`** on plans that
`apply` then rejected outright — **also closed by #488**, which gave `check` the same preconditions
`apply` runs.

## Why it matters here

This tool is how the repo is supposed to perform mechanical moves — `code-restructuring` mandates
it, and CLAUDE.md's judgment boundaries mean hand-moving code is the fallback, not the plan. While
these stand, every cross-crate extraction degrades to `git mv` plus hand-edited imports, which is
what actually happened across `#unbundle`.

## What would close it — narrowed 2026-09-18 (green #490)

**All four original refusals are gone.** #488 closed two, #490 closed the other two. What is left is
not a refusal in this tool at all, but **one consumer that opts out of the fix**:

`packages/tddy-index-daemon/src/apply.rs:45` still calls `StatePaths::under(root)`, so a plan applied
**through the index daemon** keeps the repo-scoped journal and still pays the tax FR3 removed: a
completed plan blocks the next one under that root.

The fix is **one line** — `StatePaths::for_plan(root, options.plan()?)?`, and `options.plan()?` is
already read on the line above, so nothing has to be threaded in. It was left out of #490
deliberately, and the reason is *not* that the plan path is unavailable:

- `apply_plan`'s own doc comment, the per-root queue rationale in `index.rs`, `operations.rs` and
  `queries.rs`, and the committed `packages/tddy-index-daemon/docs/code-index-service.md` all justify
  the queue partly by *"`.restructure/` is keyed by root with no lock file"*. That sentence stops
  being true, so the change is one line of code plus a documentation reconciliation across another
  package — and `packages/*/docs/` may only be edited through a changeset, never directly.
- Nothing is **broken** meanwhile. The per-root queue serializes tree access as well as journal
  access, so it is merely stricter than it now needs to be; correctness does not depend on the
  repo-scoped layout.

So: a one-line behaviour change plus a changeset for `tddy-index-daemon`'s docs. Small, but a
different package's concurrency story, which is why it is recorded rather than smuggled into a node
whose Boundaries say it changes the tool only.

This record **stays** until that consumer moves, and is deliberately not deleted: a closed record's
information survives in the changelog entry, whereas a partially-fixed one's *remainder* exists
nowhere else.

**`--indexing-budget` is already fixed** and its backlog section is stale — `request_timeout` reads
the budget, `settle_budget_for` scales the per-operation wait, and `map_lsp_error(Timeout)` maps to
a retryable state. #488 verifies and closes it rather than re-implementing it.

## If you are about to change this code

**`crate_move.rs` no longer exists as one file.** #490's Phase A carved it into
`crate_move/{moving,module_home,manifest_edits,header,refusals,cluster,destination,preconditions}.rs`
with glob facades in the parent; the file itself is 914 lines, of which ~331 are production. Any line
number cited against the old 1,998-line file is stale.

If you merely *use* the tool, none of this affects you: #490 added capability and changed no existing
operation's behaviour — `resolve` delegates to `resolve_cluster` over a one-member set, so the
single-module path and the cluster path are the same code, and the three real-server acceptance
suites that pin single-module behaviour still pass unchanged.

## Verified by hand

2026-09-18 (green #490): re-measured after #490's green phase. Both remaining refusals are closed in
the tool. Evidence: `cargo test -p tddy-code-restructuring --all-targets --no-fail-fast` → **380
passed / 0 failed across 14 targets**, up from 362/4; the four `tests/cluster_move.rs` failures are
green with that file untouched, and `move_module_to_crate_acceptance` (2), `nested_module_move_acceptance`
(4) and `facade_cycle_acceptance` (3) still pass against a real rust-analyzer. `clippy --all-targets
-D warnings` and `fmt --all --check` clean.

**Measurement caveat worth keeping:** `./test` does not pass `--no-fail-fast`, and `cluster_move` is
only the 3rd of 13 suites alphabetically, so every earlier measurement of this crate that used
`./test` while `cluster_move` was red **silently skipped nine suites**. Measure this crate with
`--no-fail-fast` or the numbers under-report it.

Not verified: a live cluster apply against a real workspace. #490's Boundaries forbid carving a crate,
so AC1 is proven by unit tests over real temp workspaces rather than by a live run; a rehearsal on a
throwaway tree is still outstanding.

2026-09-18: #488 merged. Re-checked: nested anchors and facade attribution are closed; the singular
`Move` model and `StatePaths::under` remain. Narrowed *What would close it* to those two rather than
resolving the record.

2026-09-15: confirmed all three code-level refusals still present at the cited lines, and confirmed
the fourth (`--indexing-budget`) is fixed — the backlog entry predates the fix, whose last edit was
`#unbundle 3/10` while the fix landed later. Both source todos remain the authoritative narrative;
this record is the standing measurement.
