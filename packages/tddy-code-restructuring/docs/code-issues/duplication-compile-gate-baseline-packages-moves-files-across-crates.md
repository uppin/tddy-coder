# duplication: "this operation moves files across crates" spelled out twice

**Location:** `packages/tddy-code-restructuring/src/runner/compile_gate.rs:60` — `baseline_packages`;
`packages/tddy-code-restructuring/src/runner/rehearsal.rs:169` — `untracked_move_anchors`
**Category:** duplication
**Detected:** 2026-10-17 by `/analyze-clean-code` on #607 (`#reshape` 9/19); re-derive with
`grep -rn 'moves_across_crates() || .*MoveTestBinaryToCrate' packages/tddy-code-restructuring/src`
**Metrics:** **2 copies** of the predicate `op.op.moves_across_crates() || op.op == RefactorKind::MoveTestBinaryToCrate`, both added by #607
**Restructure:** no — one method on `RefactorKind`, ordinary work
**Status:** Open — **unclaimed**
**Verified:** by hand, 2026-10-17 (both copies read; same predicate, same meaning)

## Measurement history

| Run | Copies | Note |
|---|---|---|
| 2026-10-17 | 2 | first detection; both introduced by #607 (the destination baseline and the static untracked-move check) |

## What the tool found

`RefactorKind::moves_across_crates()` covers the module and cluster moves; the test-binary move is
added beside it by hand at both sites. The two sites answer the same question — "does this
operation `git mv` a file into another crate?" — for the compile gate's destination packages and for
the static untracked-anchor finding.

## Why it matters here

A fourth cross-crate kind added to one site and not the other would make the baseline and the static
`check` disagree about which operations move files, which is the parity this code exists to keep.

## What would close it

A `RefactorKind` method in `plan/refactor_kind.rs` (for example `moves_files_across_crates`) that
both sites call, or widening `moves_across_crates` itself if no caller relies on it excluding the
test-binary move. Deferred in #607: both files are carried by later `#reshape` branches.

## Verified by hand

- 2026-10-17 (#607 wrap): `compile_gate.rs:60` (`.filter(|op| …)`) and `rehearsal.rs:169`
  (`if !(…) { … }`) hold the same two-term predicate.
