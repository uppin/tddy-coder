# oversized-file: virtual_tui.rs

**Location:** `packages/tddy-tui/src/virtual_tui.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` file length gate on PR #763 (`#term-snapshot` 4/4)
**Metrics:** **963 production lines** (2026-10-25) (counted to the first `#[cfg(test)]`; 1,414 total) · budget 500 · **1.9× over**
**Thresholds breached:** length 963 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 963 | first record of the *length*; 901 at the PR's base, **+62** in PR #763 (`#term-snapshot` 4/4): `VIRTUAL_TUI_REPAINT_REQUEST` and `take_repaint_request`, `apply_leading_control_sequences` (repaint requests and resizes at the front of the input buffer), and `resend_mouse_capture` (re-enabling mouse tracking after every full repaint, which now reaches the client led by RIS). Split deferred with the developer's consent (code issues may be deferred in `#term-snapshot`). The file already carries a complexity record, [`complexity-virtual-tui-process-virtual-tui-input-chunk.md`](complexity-virtual-tui-process-virtual-tui-input-chunk.md) |

## What the tool found

`/pr-wrap` step 3.5 counts production lines to the first `#[cfg(test)]`. The module holds the
headless terminal's render loop (`run_virtual_tui` and its `render_and_send` closure), input parsing
(resize, repaint request, SGR mouse, keys), `apply_event` and the presenter-broadcast drain.

## What would close it

Not designed. The input parsers (`parse_resize_from_buf`, `parse_mouse_from_buf`,
`parse_key_from_buf`, `take_repaint_request`) are pure functions over a byte buffer with no
dependency on the render loop, so a `virtual_tui/input.rs` is the likely first seam; the render loop
and `process_virtual_tui_input_chunk` are the second, and overlap the complexity record, so the two
should be planned as one change with `/code-restructuring`.
