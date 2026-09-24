# oversized-file: lib.rs

**Location:** `packages/tddy-session-tool-client/src/lib.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **1,047 production lines** of 1,150 total (`#[cfg(test)] mod tests` at line 1048; 103 test lines) · budget 500 · **2.1× over**. The gate's awk reports **602**. That count is wrong, because the gate's regex also matches `#[cfg(any(feature = "livekit", test))]` at L603, which guards one production function (`worktree_activity_line`), and it stops there. The true count runs to the `mod tests` marker
**Thresholds breached:** length 1,047 > 500 (the gate's undercount, 602, would also breach it)
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured, and the L603 false stop was checked by reading the file; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 1,047 (gate: 602) | first detection. The size predates #509 (1,043 at merge-base `4e7157d2`; gate: 602). #509 changed 8 lines (6+/2−) in the request construction of `connect_sandbox_ipc`: it passes `tddy_rpc::RequestTransport::UnixSocket` to `StdioEndpoint::from_duplex`, and rustfmt spread the call across lines. That grew the true production count by **4 lines**. The gate missed the growth because it falls after the false stop |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | LiveKit transport: `LiveKitRoomKey`, `LiveKitRoomCache`, `LiveKitSession` and their impls, `livekit_room_cache`, `connect_livekit_client`, `spawn_worktree_activity_log`, `worktree_activity_line`, both `dispatch_via_livekit` variants, `livekit_session` (L419–700, L788–793) | ~285 |
| B | RPC transports: `NoCallbackToolService`, `dispatch_via_sandbox_ipc`, `dispatch_via_daemon_uds`, `connect_sandbox_ipc`, `dispatch_via_daemon_http`, `execute_tool_request`, `dispatch_via_rpc_transport`, `dispatch_via_streaming_rpc` (L702–1046, less L788–793) | ~335 |
| C | remote-block clamping: `MAX_REMOTE_BLOCK_MS`, `clamp_remote_block_ms`, `remote_block_arg_keys`, `requested_block_ms`, `clamp_remote_blocking_args` (L105–204) | ~100 |
| — | stays: `SessionToolTransport`, `SessionToolEnvelope`, transport detection, `format_tool_dispatch_result`, `dispatch_session_tool` | — |

From this first reading, A+B take the root to about 430 lines. Everything here is a free item, so no delegating stubs are needed, but `pub` re-exports keep the crate's surface. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. No dependent is known to touch this file.
