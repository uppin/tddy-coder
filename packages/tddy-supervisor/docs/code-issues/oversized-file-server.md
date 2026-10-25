# oversized-file: server.rs

**Location:** `packages/tddy-supervisor/src/server.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 — `/pr-wrap` step 3.5 file-length gate on #739 (`#start-progress` 6/8), production lines counted to the first `#[cfg(test)]`
**Metrics:** **796 production lines** · budget 500 · already over at merge-base (708)
**Thresholds breached:** length 796 > 500
**Restructure:** not designed — the `PrivilegedSurface` handlers (spawn, sandbox, clone, scopes) are separable from the RPC adapter and `serve`
**Status:** Closed 2026-10-25 by #721's split (measured after the rebase onto #start-progress) — under budget; delete at #721's wrap, recording this final measurement in its change-history entry
**Verified:** ⚠ **not hand-verified** — machine-measured line count

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-24 | 708 → 796 | grown by #739: `PrivilegedSurface::clone_repository`, `resolve_spawn_of`, the `CloneRepository` adapter |
| 2026-10-25 | 796 → 392 | closed by #721 (`#coagents` 2/8), rebased onto #start-progress: its split moved the RPC adapter to `server/rpc.rs` (203) and `serve` / the listener to `server/listener.rs` (300), and `clone_repository`'s adapter went with the RPC adapter |
