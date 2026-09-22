# oversized-file: callbacks.rs — a vocabulary module under a handler name

**Location:** `packages/tddy-telegram-control/src/telegram_session_control/callbacks.rs`
**Category:** oversized-file
**Detected:** 2026-09-22 by the `/pr-wrap` file-length gate and `/analyze-clean-code` on #494 (`#carve` 8/11)
**Metrics:** **736 production lines** (1,232 total) · budget 500 · **+236**, measured to the first `#[cfg(test)]`
**Thresholds breached:** length 736 > 500
**Restructure:** required — see *What would close it*
**Status:** Open — **unclaimed**; produced by a move-only split, deferral consented by the developer (`docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-22 | 736 | first detection — created by #494's split of `telegram_session_control.rs` |

## What the tool found

**736 production lines** (1,232 total; the `#[cfg(test)] mod unit_tests` opens at `:737`). No
handler lives here: the module holds the slash-command constants (`START_WORKFLOW_CMD`, `/sessions`,
`/delete`, `/answer-*`), the callback-data prefixes (`CB_*`), about 30 pure `parse_*` functions,
the model and recipe catalogues, text chunking and session-list formatting — plus two items that
are neither: `resolve_child_grpc_port` (`:353`, a session-registry lookup) and
`session_list_status_or_placeholders` (`:391`, a filesystem read). Seven of its items are
`pub(super)` because a sibling uses them.

## Why it matters here

The name says the callback *handlers* are here; they are in the other five modules. A reader
looking for a callback's handling opens the wrong file first, and the module is the one every
sibling reaches through `use super::*`.

`#carve` 8/11 (#494) split the unsplit 3,980-line `telegram_session_control.rs` into seven
modules by line range, as a **move-only** step: the node's boundary forbade decomposing any
handler, and the split was pinned against an 800-line ceiling (AC5), not the repo's 500. So every
module is a faithful slice of the old file, and five of them landed over budget.

## What would close it

Move-only, in this order:

1. **Rename** it to `vocabulary.rs` (what its own header calls it) or `protocol.rs`. The rename
   touches only `mod.rs`'s `mod`/`pub use` lines, because children reach it through `use super::*`.
2. **Move its unit tests** to `callbacks/tests.rs` (or `vocabulary/tests.rs`). That does not move the
   production count, but it halves the file a reader opens.
3. **Split the production half** into `commands.rs` (slash commands and their parsers) and
   `callback_data.rs` (prefixes, their parsers and `map_elicitation_callback_to_presenter_input`),
   about 370 lines each.
4. Move `session_list_status_or_placeholders` next to the session list (see
   `oversized-file-telegram-session-control-chaining-and-listing.md`), which makes it private again
   and takes the only filesystem read out of the vocabulary module.

## Verified by hand

2026-09-22: counted production lines to the first `#[cfg(test)]` and confirmed it opens the
file's test module rather than a `#[cfg(test)] use`. Seam notes are from `/analyze-clean-code` on
#494 and were checked against the item list of the file.
