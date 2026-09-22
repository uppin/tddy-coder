# oversized-file: pickers.rs — the picker chain and the workflow spawn

**Location:** `packages/tddy-telegram-control/src/telegram_session_control/pickers.rs`
**Category:** oversized-file
**Detected:** 2026-09-22 by the `/pr-wrap` file-length gate and `/analyze-clean-code` on #494 (`#carve` 8/11)
**Metrics:** **705 production lines** (no `#[cfg(test)]` module) · budget 500 · **+205**, measured to the first `#[cfg(test)]`
**Thresholds breached:** length 705 > 500
**Restructure:** required — see *What would close it*
**Status:** Open — **unclaimed**; produced by a move-only split, deferral consented by the developer (`docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-22 | 705 | first detection — created by #494's split of `telegram_session_control.rs` |

## What the tool found

**705 production lines**, no test module. The keyboard picker chain — intent → project → branch
(and branch-more) → conflict → model → agent — plus `spawn_telegram_workflow` (`:618`), the
Telegram-started spawn path, which calls
`merge_chain_integration_base_with_explicit_operator_overrides` (`:658`) and the presenter
observer. `handle_telegram_branch_callback` (`:220`) is **177 lines** on its own and has its own
record, `complexity-telegram-session-control-handle-telegram-branch-callback.md`.

## Why it matters here

Every Telegram session start walks this chain, so it is the module most operator-facing changes
touch; the one long handler makes each such change a read of 177 lines.

`#carve` 8/11 (#494) split the unsplit 3,980-line `telegram_session_control.rs` into seven
modules by line range, as a **move-only** step: the node's boundary forbade decomposing any
handler, and the split was pinned against an 800-line ceiling (AC5), not the repo's 500. So every
module is a faithful slice of the old file, and five of them landed over budget.

## What would close it

Two moves, one move-only and one not:

- **Move-only:** `spawn_telegram_workflow` next to the rest of the spawn helpers in
  `workflow_spawn.rs` (375 lines, room for it), which takes this file to about 620 and `workflow_spawn.rs` to about 460.
- **Decomposition:** `handle_telegram_branch_callback` by `extract_method` along its branch
  structure — the fix its own complexity record describes. With that, the file goes under 500.

## Verified by hand

2026-09-22: counted production lines to the first `#[cfg(test)]` and confirmed it opens the
file's test module rather than a `#[cfg(test)] use`. Seam notes are from `/analyze-clean-code` on
#494 and were checked against the item list of the file.
