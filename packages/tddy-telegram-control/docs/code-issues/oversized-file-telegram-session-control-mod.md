# oversized-file: telegram_session_control/mod.rs — DTOs, the harness and a glob-import hub

**Location:** `packages/tddy-telegram-control/src/telegram_session_control/mod.rs`
**Category:** oversized-file
**Detected:** 2026-09-22 by the `/pr-wrap` file-length gate and `/analyze-clean-code` on #494 (`#carve` 8/11)
**Metrics:** **539 production lines** (no `#[cfg(test)]` module) · budget 500 · **+39**, measured to the first `#[cfg(test)]`
**Thresholds breached:** length 539 > 500
**Restructure:** required — see *What would close it*
**Status:** Open — **unclaimed**; produced by a move-only split, deferral consented by the developer (`docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-22 | 539 | first detection — created by #494's split of `telegram_session_control.rs` |

## What the tool found

**539 production lines**, no test module. About 45 imports that all six children receive through
`use super::*`; about 150 lines of public request/outcome DTOs (lines ~51–200:
`StartWorkflowCommand` … `EnterSessionOutcome`, `ChangesetRoutingSnapshot`,
`PresenterInputPayload`); the `TelegramSessionControlHarness` struct and its constructors; and the
two glob re-exports `pub use callbacks::*; pub use workflow_spawn::*;` that keep every pre-split
`telegram_session_control::X` path resolving.

## Why it matters here

The globs were the right call for a move-only node — no consumer changed — but they hide which
child owns each symbol and which child depends on what, and that is what the next split has to
work out by hand.

`#carve` 8/11 (#494) split the unsplit 3,980-line `telegram_session_control.rs` into seven
modules by line range, as a **move-only** step: the node's boundary forbade decomposing any
handler, and the split was pinned against an 800-line ceiling (AC5), not the repo's 500. So every
module is a faithful slice of the old file, and five of them landed over budget.

## What would close it

Move-only:

- Move the DTOs into `types.rs` — about 150 lines out, taking `mod.rs` to about 390. This alone
  closes the record.
- Then, as hygiene rather than budget: replace each child's `use super::*` with explicit imports
  (rust-analyzer's "expand glob import" does it mechanically) and the two glob `pub use`s with
  explicit re-export lists.

## Verified by hand

2026-09-22: counted production lines to the first `#[cfg(test)]` and confirmed it opens the
file's test module rather than a `#[cfg(test)] use`. Seam notes are from `/analyze-clean-code` on
#494 and were checked against the item list of the file.
