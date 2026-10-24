# oversized-file: server.rs

**Location:** `packages/tddy-supervisor/src/server.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 — `/pr-wrap` step 3.5 file-length gate on #739 (`#start-progress` 6/8), production lines counted to the first `#[cfg(test)]`
**Metrics:** **796 production lines** · budget 500 · already over at merge-base (708)
**Thresholds breached:** length 796 > 500
**Restructure:** not designed — the `PrivilegedSurface` handlers (spawn, sandbox, clone, scopes) are separable from the RPC adapter and `serve`
**Status:** Open — **unclaimed** — split deferred with the developer's consent, #start-progress 6/8. The file is touched by other `#start-progress` nodes, so a split here would cascade rename conflicts through the stack; do it after the stack lands.
**Verified:** ⚠ **not hand-verified** — machine-measured line count

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-24 | 708 → 796 | grown by #739: `PrivilegedSurface::clone_repository`, `resolve_spawn_of`, the `CloneRepository` adapter |
