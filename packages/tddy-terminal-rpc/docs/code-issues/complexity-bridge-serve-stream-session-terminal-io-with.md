# complexity: serve_stream_session_terminal_io_with

**Location:** `packages/tddy-terminal-rpc/src/bridge.rs` — `serve_stream_session_terminal_io_with`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` on PR #760 (`#term-snapshot` 1/4), brace-depth scan
**Metrics:** **74 lines** · **nesting depth 6**
**Thresholds breached:** length 74 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-10-25 | 74 | 6 | first detection; 61 lines / nesting 5 on `origin/master`, grown in PR #760 by the input refusal frame (a refused chunk is answered on the output half and the forwarder carries on). Deferred with the developer's consent: `bridge.rs` is stack-shared with #761–#763 |

## What the tool found

The bidi entry point opens the output half and then spawns the input forwarder inline; the
forwarder's control check, refusal send and stdin write sit inside `tokio::spawn` → `while let` →
`match` → `if`.

## What would close it

Extract the input forwarder (`tokio::spawn(async move { … })`) into its own function taking the
session, the control gate, the frame identity and the weak output sender. That alone brings both the
length and the nesting under threshold.
