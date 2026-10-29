# oversized-file: terminal_capture.rs

**Location:** `packages/tddy-task/src/terminal_capture.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 on PR #760 (`#term-snapshot` 1/4)
**Metrics:** **712 production lines** (2026-10-29; no `#[cfg(test)]` block; the file's tests live in `tests/`) · budget 500 · **1.3× over**
**Thresholds breached:** length 668 > 500
**Restructure:** `extract_module --to_file` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Related:** `complexity-terminal-capture-feed.md` (`EscapeParser::feed`, in the same file)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 668 | first detection; 482 → 668 in PR #760: the screen model (`avt::Vt`, incremental UTF-8 feed), `ScreenSnapshot`, `InputModes` (the widened sniffer state) and the DECKPAM/DECSCUSR parser events. Decomposition deferred with the developer's consent: the file is stack-shared — `#term-snapshot` 2/4–4/4 (#761, #762, #763) modify it too, and a split here would conflict in each |
| 2026-10-28 | 728 | **regressed.** `#home-inbox` 2/8 (#767): 668 → 728 (+60) — `TerminalCapture::text_grid` (the active buffer's rows, cursor, size, offset and the bracketed-paste state) with `TextGrid` / `GridCursor`. The accessor reads the screen model and belongs with `snapshot` in the screen-model seam above. **Split deferred** (stack-shared: `#home-inbox` 5/8 and 6/8 touch the file) |
| 2026-10-29 | 712 | `#home-inbox` 9/9 (#785): the styles walk and crop went into `terminal_capture/styles.rs` and `crop.rs`, and `TextGrid` / `GridCursor` moved to `grid.rs`; net −16 against 728 despite the module wiring and two `TextGrid` fields. The remaining split (sniffer and mode state) is unchanged |

## What the tool found

Three concerns share one file: the raw capture ring with its offset bookkeeping and eviction
(`TerminalCapture` itself), the escape-sequence sniffer (`EscapeParser`, `StreamEvent`,
`PrivateModeUpdate`) and the sticky input-mode state it feeds (`InputModes`), and the screen model's
glue (`feed_screen`, `snapshot`, `new_screen`).

## Why it matters here

Every terminal wire reads through this type. The parser and the mode state change for reasons
unrelated to the ring's offset arithmetic, so a reader changing one has to page through the others.

## What would close it

Move `EscapeParser` with `StreamEvent` / `PrivateModeUpdate` / `ParserState` and `InputModes` into a
sibling module (`terminal_capture/modes.rs` or `escape_sniffer.rs`), leaving the ring and the
snapshot on `TerminalCapture`. Anchor and prove the seam with `tddy-tools restructure`, after the
`#term-snapshot` stack lands.
