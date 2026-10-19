# oversized-file: sync.rs

**Location:** `packages/tddy-session-sync/src/sync.rs`
**Category:** oversized-file
**Detected:** 2026-10-19 — `/pr-wrap` file-length gate on #629 (`#satellite` 7/13)
**Metrics:** **788 production lines** (2026-10-19, #629 HEAD; 788 on `origin/master`) (first `#[cfg(test)]` counted by `awk '/^[[:space:]]*#\[cfg\(.*test[),]/{exit} {n++} END{print n}'`) · budget 500 · **1.58× over**
**Thresholds breached:** length 788 > 500
**Restructure:** required — seams not yet designed
**Status:** Open — **unclaimed** · pre-existing; #629 touched it (the RPC client became `Arc<dyn RpcClientTransport>`, payloads became plain bytes) and left the production-line count unchanged

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-19 | 788 → 788 (unchanged) | `#satellite` 7/13 (#629): `use tddy_livekit::RpcClient` leaves, `Arc<dyn tddy_rpc::RpcClientTransport>` and `std::sync::Arc` arrive, and `Syncer::open` takes the room session's `rpc()`. Not grown by #629; first record of this file |

## What would close it

Split `run`/`Syncer` (the event loop and its per-topic handlers) from the git-transport and mirror
helpers. Derive the seams with `restructure anchors` first.
