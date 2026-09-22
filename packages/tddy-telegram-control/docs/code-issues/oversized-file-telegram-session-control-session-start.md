# oversized-file: session_start.rs — two CLI spawns carry the size

**Location:** `packages/tddy-telegram-control/src/telegram_session_control/session_start.rs`
**Category:** oversized-file
**Detected:** 2026-09-22 by the `/pr-wrap` file-length gate and `/analyze-clean-code` on #494 (`#carve` 8/11)
**Metrics:** **711 production lines** (no `#[cfg(test)]` module) · budget 500 · **+211**, measured to the first `#[cfg(test)]`
**Thresholds breached:** length 711 > 500
**Restructure:** required — see *What would close it*
**Status:** Open — **unclaimed**; produced by a move-only split, deferral consented by the developer (`docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-22 | 711 | first detection — created by #494's split of `telegram_session_control.rs` |

## What the tool found

**711 production lines**, no test module. `/start-workflow`, `/start-claude` and `/start-cursor`:
session creation, the model-pick callbacks, `persist_changeset_model` /
`persist_changeset_session_type_marker` (both `pub(super)`, used by `pickers`), and the two CLI
spawns — `spawn_telegram_claude_cli` (`:331`, **217 lines**) and `spawn_telegram_cursor_cli`
(`:574`, **137 lines**). Those two functions are half the file.

## Why it matters here

The size is not a seam problem: it is two long functions. A module split alone would move them
intact and leave one of the halves over budget.

`#carve` 8/11 (#494) split the unsplit 3,980-line `telegram_session_control.rs` into seven
modules by line range, as a **move-only** step: the node's boundary forbade decomposing any
handler, and the split was pinned against an 800-line ceiling (AC5), not the repo's 500. So every
module is a faithful slice of the old file, and five of them landed over budget.

## What would close it

Decompose the two spawns — `extract_method` along their phases (worktree and changeset setup,
argv build, PTY spawn, registry and observer wiring) — which takes both under the 60-line ceiling
and the file under 500. That is a behaviour-risk change and needs the spawns under test first; it
was out of #494's move-only boundary. Moving the model-pick keyboards here from `pickers.rs` would
remove no widening, because `pickers` also calls the two `persist_*` helpers.

## Verified by hand

2026-09-22: counted production lines to the first `#[cfg(test)]` and confirmed it opens the
file's test module rather than a `#[cfg(test)] use`. Seam notes are from `/analyze-clean-code` on
#494 and were checked against the item list of the file.
