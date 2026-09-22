# 2026-09-22 — The Telegram control plane left `tddy-session-lifecycle` with five modules over budget

**Category:** Deferred from `carve-telegram` (#494, `#carve` 8/11)
**Source:** `/pr-wrap` file-length gate and `/analyze-clean-code` on #494; change history
`docs/dev/changesets/2026-09-22-carve-telegram.md`
**Status:** open — **deferral consented by the developer** at `/pr-wrap`, 2026-09-22 ("move-only
node; developer consent 2026-09-22 in /pr-wrap")

#494 split the 3,980-line `telegram_session_control.rs` into seven modules and moved them, with the
rest of the Telegram cluster, into the new `tddy-telegram-control` crate. The split was a
**line-range slice** pinned to an 800-line ceiling (AC5), not the repo's 500, and five modules
landed over 500. Two files that were already over budget also grew by the move:

| File (`packages/…`) | Production lines | Over by | Record |
|---|---:|---:|---|
| `tddy-telegram-control/src/telegram_session_control/callbacks.rs` | 736 | 236 | `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-callbacks.md` |
| `…/telegram_session_control/session_start.rs` | 711 | 211 | `…/oversized-file-telegram-session-control-session-start.md` |
| `…/telegram_session_control/pickers.rs` | 705 | 205 | `…/oversized-file-telegram-session-control-pickers.md` |
| `…/telegram_session_control/mod.rs` | 539 | 39 | `…/oversized-file-telegram-session-control-mod.md` |
| `…/telegram_session_control/chaining_and_listing.rs` | 518 | 18 | `…/oversized-file-telegram-session-control-chaining-and-listing.md` |
| `tddy-telegram-control/src/telegram_notifier.rs` | 1,239 → 1,246 | +7 by #494 | `…/oversized-file-telegram-notifier.md` |
| `tddy-daemon/src/runtime.rs` | 1,519 → 1,521 | +2 by #494 | `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` |

## Why it was deferred

- **The node is move-only.** Its boundary forbids decomposing any handler, changing log targets
  or messages, or editing what the moved suites assert. Three of the five modules
  (`session_start.rs`, `pickers.rs`, and `callbacks.rs`' production half) cannot reach 500 by moving
  items alone: the size is in long functions — two CLI spawns of 217 and 137 lines, a 177-line
  branch callback — whose decomposition is behaviour-risk work on untested code.
- **The developer chose to keep the seven modules** rather than re-split them in #494, so the diff
  under review stays a verifiable slice of the old file (its sorted-line multiset is checked against
  the original in the commit message).
- `telegram_notifier.rs`' +7 and `runtime.rs`' +2 are `rustfmt` rewraps of re-pointed paths and
  the one port-injection cast. Neither file's oversize is new.

## What would close it

Each record's *What would close it* has the seams. In short, all move-only except where noted:
rename `callbacks` to `vocabulary` and split it into commands and callback data; move the DTOs from
`mod.rs` into `types.rs`; gather the recipe cluster from `chaining_and_listing.rs` and
`elicitation.rs` into `recipes.rs` and split the rest into `chaining.rs` and `session_list.rs`;
move `spawn_telegram_workflow` into `workflow_spawn.rs`; and — **not** move-only — decompose the two
CLI spawns and `handle_telegram_branch_callback`. `telegram_notifier.rs` and `runtime.rs` stay with
their own records.

Close this entry when the five `oversized-file-telegram-session-control-*` records are gone.
