# complexity: spawn_live_bridge

**Location:** `packages/tddy-terminal-rpc/src/bridge.rs` — `spawn_live_bridge`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` on PR #760 (`#term-snapshot` 1/4), brace-depth scan
**Metrics:** **55 lines** · **nesting depth 6**
**Thresholds breached:** nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-10-25 | 55 | 6 | first detection; 47 lines / nesting 6 on `origin/master`, +8 lines in PR #760 (offset stamping of live frames in a `SNAPSHOT` stream). Deferred with the developer's consent: `bridge.rs` is stack-shared with #761–#763 |

## What the tool found

A spawned task with a `loop { tokio::select! { … match … } }` — the stdout arm, the ACK arm and the
`pty_done` arm each nest three levels inside the task body.

## What would close it

Extract each `select!` arm's body (forward a stdout chunk with its stamp, forward an ACK) into a
function on a small bridge-state struct, so the loop reads as three calls.
