# oversized-file: pty_relay.rs

**Location:** `packages/tddy-terminal-rpc/src/pty_relay.rs`
**Category:** oversized-file
**Detected:** 2026-10-17 — `/pr-wrap` file-length gate on #623 (`#satellite` 1/13)
**Metrics:** **784 production lines** (2026-10-19, #629; 776 at #623) (first `#[cfg(test)]` counted by `awk '/#\[cfg\(.*test[),]/{exit} {n++} END{print n}'`) · budget 500 · **1.57× over**
**Thresholds breached:** length 776 > 500
**Restructure:** required — seams not yet designed
**Status:** Open — **unclaimed** · pre-existing; #623 grew it by 2, #629 by 8 (deferred with the developer's consent, 2026-10-19 — `docs/dev/todo/2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-17 | 774 → 776 (+2) | `#satellite` 1/13 (#623): `connect_session_over_http` becomes `open_session_room_over_http`, which also decodes the reply. Already over budget on `master`; split deferred with the developer's consent — `docs/dev/todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md` |
| 2026-10-19 | 776 → 784 (+8) | `#satellite` 7/13 (#629): the LiveKit relay path joins through `RoomConnector` — `join_terminal_room` (the room request and error mapping), three constants for what were inline literals, and the connector parameter on `run_pty_relay` arrive; the SDK-typed token mint, `connect_client` call, `start_bidi_stream` handling and the `livekit` feature gates leave. The `livekit` cargo feature is gone, so `open_session_room_over_http` and `encode_resize` are compiled unconditionally. Split deferred with the developer's consent (2026-10-19) — `docs/dev/todo/2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget.md` |

## What would close it

Split the LiveKit relay path (`run_livekit_session`, `run_livekit_terminal`, the Connect-HTTP helpers) from
the local relay. Re-derive the seams with `restructure anchors` first.
