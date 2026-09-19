# oversized-file: backends/rust.rs — the whole rust-analyzer backend in one module

**Location:** `packages/tddy-code-restructuring/src/backends/rust.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by the `/pr-wrap` file-length gate on #498
**Metrics:** **4,788 production lines** (4,772 before #498) · budget 500
**Restructure:** required
**Status:** Open — pre-existing; #498 added 16 lines and deferred, blocked by stack overlap

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 4,788 | first detection; 4,772 before #498 |

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
