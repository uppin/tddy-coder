# oversized-file: cursor.rs

**Location:** `packages/tddy-agent-backend/src/backend/cursor.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **519 production lines** of 707 total (`#[cfg(test)]` at line 520; 188 test lines) · budget 500 · **1.0× over**
**Thresholds breached:** length 519 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/backend/cursor.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 519 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: no line of the file changed |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | argv and model list: `parse_cursor_model_list`, `build_cursor_cli_args`, `register_cursor_mcp_config` (L146–228) | ~85 |
| B | the second `impl CursorBackend` block, the invoke path (L229–519) | ~290 |

19 lines over. Seam A alone brings it under budget. Prove the seams with `restructure check --deep` before applying them.
