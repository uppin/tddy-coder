# oversized-file: listener.rs

**Location:** `packages/tddy-toolcall/src/toolcall/listener.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **671 production lines** of 1,055 total (`#[cfg(test)]` at line 672; 384 test lines) · budget 500 · **1.3× over**
**Thresholds breached:** length 671 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/toolcall/listener.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 671 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: no line of the file changed |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | listener start-up: the spawn-handler traits, the toolcall log, both `start_toolcall_listener` variants, `start_toolcall_listener_with_conversation_handler`, `accept_loop` (L23–205) | ~185 |
| B | `impl ToolcallRpcService`, split by RPC family: submit/ask/approve, actions, build (L206–627) | ~420 |
| C | `handle_build_request` (L628–671) | ~45 |

Seam A alone brings it to about 490 lines. Prove the seams with `restructure check --deep` before applying them.
