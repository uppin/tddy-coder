# oversized-file: exec_tool/project_home.rs

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 file-length gate on #753 (`#start-parity` 4/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **665 production lines** · budget 500 · 1.3× over
**Thresholds breached:** length 665 > 500
**Restructure:** not designed
**Status:** Open — split deferred: the file is stack-shared (every `#start-parity` node implements its project-home port methods here), and a split mid-stack would conflict in every later node; the follow-up is after the stack lands — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 665 | first detection. `#start-parity` 4/9 (#753): 478 → 665 (+187) — `start_ledger_of` with its once-per-process recovery, the `start_session` / `get_session_start` / `cancel_session_start` port methods (local and forwarded cancel), and `StartedSessionListings` (the started session's branch and worktree, local or from a peer). **Split deferred by developer instruction** (stack-shared) |

## What would close it — candidate seams (not proven)

| Seam | Items |
|---|---|
| A | the start methods and `StartedSessionListings` → `exec_tool/project_home_start.rs` |
| B | the session listing (`rows`, `session_rows`, `list_sessions`, `list_peer_sessions`) |
| C | the caller resolution and tool-call log (`project_home_caller`, `record_call`) |
