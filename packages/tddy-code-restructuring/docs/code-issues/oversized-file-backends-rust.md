# oversized-file: backends/rust.rs — the whole rust-analyzer backend in one module

**Location:** `packages/tddy-code-restructuring/src/backends/rust.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by the `/pr-wrap` file-length gate on #498
**Metrics:** **4,342 production lines** (4,788 before #527) · budget 500
**Restructure:** required
**Status:** Open — pre-existing; #498 added 16 lines and deferred, blocked by stack overlap; **partially fixed** by #527 (−472 net); the decomposition below still stands

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 4,788 | first detection; 4,772 before #498 |
| 2026-09-23 | 4,294 | #527 (engine fixes) changed the import pass, the `impl`-seam refusal and the readiness waits, and moved each into a sibling under `backends/rust/` rather than growing this file: `imports.rs`, `impl_seam.rs`, `chatter.rs` (`ServerChatter`, re-exported at its old path), `readiness.rs`. Still 8.6× the budget |
| 2026-09-24 | 4,307 | #527's three explicit-failure guards. The logic went to siblings (`early_return.rs`, `chatter.rs`, `readiness.rs`, and `runner/compile_gate.rs` outside this file); the +13 here is wiring only: the `mod`/`use` lines and the two `refuse_early_returns` call sites in `check` and `resolve` |
| 2026-09-24 | 4,316 | #527 wrap re-measure, after gaps A–C: the repairs went to `imports.rs`, `impl_seam.rs` and a new `nested_modules.rs`; the +9 here is the `nested_modules` wiring and one reworded refusal. 8.6× the budget — `rust.rs` shrank only because new logic went elsewhere, and none of its own seams were cut |
| 2026-09-25 | 4,342 | #524 (`#carve` 14/15), `3714a654`: every entry point now closes the documents it opened. `did_open` and the closing went to a new sibling, `documents.rs` (59 lines); the +29 here (4,313 → 4,342 at `3714a654^` and after, both by the inline-test-block rule) is the three `closing_what_it_opens` wrappers around the bodies moved verbatim into `resolve_opening`, `anchor_opening` and `outside_references_opening`, and the `opened` field. Unchanged in kind: none of its own seams were cut |

## What the gate found

Already 9.5× the budget before #498. Its contribution is **16 lines**: `MoveTestBinaryToCrate` added
to the `SUPPORTED` kinds and one dispatch branch, placed before `self.start()` because the operation
needs no language server.

## Why it was not split in #498

The `/pr-wrap` stack-overlap stop. **#491 (`#carve` 5/10 `core-foundations`) also touches this file**,
and it is stacked directly on #498 — splitting the file here would rewrite paths under a PR already
in flight and turn its diff into a conflict.

## What would close it

A decomposition on a follow-up branch **after the `#carve` stack lands**, not inside it. The obvious
seams are the assist-driven operations, the hand-authored cross-crate moves, and the LSP session
plumbing.
