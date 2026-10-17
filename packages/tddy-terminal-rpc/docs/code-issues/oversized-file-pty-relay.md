# oversized-file: pty_relay.rs

**Location:** `packages/tddy-terminal-rpc/src/pty_relay.rs`
**Category:** oversized-file
**Detected:** 2026-10-17 — `/pr-wrap` file-length gate on #623 (`#satellite` 1/13)
**Metrics:** **776 production lines** (first `#[cfg(test)]` counted by `awk '/#\[cfg\(.*test[),]/{exit} {n++} END{print n}'`) · budget 500 · **1.55× over**
**Thresholds breached:** length 776 > 500
**Restructure:** required — seams not yet designed
**Status:** Open — **unclaimed** · pre-existing; #623 grew it by 2

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-17 | 774 → 776 (+2) | `#satellite` 1/13 (#623): `connect_session_over_http` becomes `open_session_room_over_http`, which also decodes the reply. Already over budget on `master`; split deferred with the developer's consent — `docs/dev/todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md` |

## What would close it

Split the LiveKit relay path (`run_livekit_session`, `run_livekit_terminal`, the Connect-HTTP helpers) from
the local relay. Re-derive the seams with `restructure anchors` first.
