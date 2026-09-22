# oversized-file: chaining_and_listing.rs — two jobs, plus three handlers of neither

**Location:** `packages/tddy-telegram-control/src/telegram_session_control/chaining_and_listing.rs`
**Category:** oversized-file
**Detected:** 2026-09-22 by the `/pr-wrap` file-length gate and `/analyze-clean-code` on #494 (`#carve` 8/11)
**Metrics:** **518 production lines** (no `#[cfg(test)]` module) · budget 500 · **+18**, measured to the first `#[cfg(test)]`
**Thresholds breached:** length 518 > 500
**Restructure:** required — see *What would close it*
**Status:** Open — **unclaimed**; produced by a move-only split, deferral consented by the developer (`docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-22 | 518 | first detection — created by #494's split of `telegram_session_control.rs` |

## What the tool found

**518 production lines**, no test module. The "and" admits two responsibilities — chaining
(`handle_chain_workflow` `:9`, 115 lines; `handle_chain_parent_callback`) and the session list
(`handle_list_sessions`, `handle_enter_session`, `handle_delete_session`) — and the module also holds
three handlers that are neither: `handle_recipe_callback` (`:196`), `handle_plan_review_phase`
(`:261`) and `handle_start_workflow_unauthorized` (`:495`). The recipe cluster is split with
`elicitation.rs`, whose `persist_recipe_to_changeset` (`:399`) and `send_more_recipes_keyboard`
(`:435`) are `pub(super)` only because their one consumer is `handle_recipe_callback` here.

## Why it matters here

Only 18 lines over budget, but the seams are wrong: two widenings exist purely because the recipe
code landed in two files by line position.

`#carve` 8/11 (#494) split the unsplit 3,980-line `telegram_session_control.rs` into seven
modules by line range, as a **move-only** step: the node's boundary forbade decomposing any
handler, and the split was pinned against an 800-line ceiling (AC5), not the repo's 500. So every
module is a faithful slice of the old file, and five of them landed over budget.

## What would close it

Move-only:

- `handle_start_workflow_unauthorized` into `session_start.rs`, beside `handle_start_workflow`.
- A `recipes.rs` holding `handle_recipe_callback`, `persist_recipe_to_changeset`,
  `send_more_recipes_keyboard` and, optionally, `handle_plan_review_phase`. Both `elicitation.rs`
  helpers become private again (two fewer widenings).
- What is left splits into `chaining.rs` and `session_list.rs`, about 200 lines each;
  `session_list_status_or_placeholders` moves from `callbacks.rs` into `session_list.rs` and
  becomes private (one fewer widening).

## Verified by hand

2026-09-22: counted production lines to the first `#[cfg(test)]` and confirmed it opens the
file's test module rather than a `#[cfg(test)] use`. Seam notes are from `/analyze-clean-code` on
#494 and were checked against the item list of the file.
