# complexity: preflight_moves, commit_operation and GroupRun::settle — one operation's context passed loose

**Location:** `packages/tddy-code-restructuring/src/runner.rs:121` — `preflight_moves`; with
`runner.rs:69` — `commit_operation` and `src/runner/group_gate.rs:229` — `GroupRun::settle`
**Category:** complexity
**Detected:** 2026-10-17 by `/analyze-clean-code` on #607 (`#reshape` 9/19); re-derive by counting
the parameters of each `fn` signature (`grep -n -A10 'pub fn preflight_moves\|pub fn commit_operation' src/runner.rs`,
`grep -n -A9 'pub fn settle' src/runner/group_gate.rs`)
**Metrics:** `preflight_moves` **6 parameters** (new in #607) · `commit_operation` **8** · `GroupRun::settle` **7** · threshold 5 (`/analyze-clean-code`)
**Thresholds breached:** parameters 6, 8 and 7 > 5
**Restructure:** no — a signature change across 4 files (`runner.rs`, `group_gate.rs`, `store_run.rs`, `tddy-index-daemon/src/apply.rs`), ordinary work
**Status:** Open — **unclaimed**
**Verified:** by hand, 2026-10-17 (the signatures read as listed below)

## Measurement history

| Run | `preflight_moves` | `commit_operation` | `GroupRun::settle` | Note |
|---|---|---|---|---|
| 2026-10-17 | 6 | 8 | 7 | first detection. #607 added `preflight_moves` (`index, id, resolved, root, overlay, spawns`); the other two are pre-existing and #607 did not change their arity |
| 2026-10-22 | 6 | 8 | 7 | unchanged. #613 (`#reshape` 18/24) edited the call sites in `store_run.rs` (the loop state is `held.…`), not the signatures |

## What the tool found

The three functions each take a slice of the same per-operation context: the plan index, the op id,
the `Resolution`, the root, and the run's `SpawnRecorder` (with the overlay, the state paths, the
journal and the ledger beside them). `preflight_moves` is the one new must-refactor item of #607's
own code; the other two are where the same context already travels loose.

## Why it matters here

Every new per-operation fact (the operation context on spawn records was the latest) has to be
threaded through all three call chains, in both apply loops (the CLI's `store_run.rs` and the index
daemon's `apply.rs`), and a positional `usize` / `Option<&OpId>` pair is easy to pass in the wrong
order.

## What would close it

One operation-context struct (index, id, resolved, root, spawns scoped by `for_operation`) shared by
`preflight_moves`, `commit_operation` and `GroupRun::settle`, so each takes at most 5 parameters.
Deferred in #607 because every touched file is also carried by later `#reshape` branches, and the
stack rule allowed only in-place helper, local and constant edits there.

## Verified by hand

- 2026-10-17 (#607 wrap): `preflight_moves(index, id, resolved, root, overlay, spawns)`,
  `commit_operation(index, id, resolved, root, paths, journal, ledger, spawns)`,
  `settle(slot, index, resolved, gate, spawns, journal, on_check)`.
