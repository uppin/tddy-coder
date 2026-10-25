# oversized-file: roster/stream.rs

**Location:** `packages/tddy-discovery/src/roster/stream.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 on `#coagents` 6/8 (#725), production lines counted to the first `#[cfg(test)]` (the file has none, so every line counts)
**Metrics:** **545 production lines** · budget 500
**Thresholds breached:** length 545 > 500
**Restructure:** `move_item` / `extract_module` — `/code-restructuring` territory
**Status:** Open — unclaimed; already over budget before #725, which shrank it
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-10-25 | 551 → 545 | first detection; **−6** in #725: the `DaemonHttp` arm of `decide_roster_subscription` joins the accepted transports, and its refusal branch and `TODO(session-agent-roster)` go. Already over 500 before this PR |

## What would close it

Split the subscription decision (`decide_roster_subscription` and its refusal reasons) from the
reconnect-forever subscription loop into sibling modules under `roster/`, until each is under 500.
