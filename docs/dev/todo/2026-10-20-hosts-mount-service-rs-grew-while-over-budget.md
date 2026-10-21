# 2026-10-20 — `service.rs` grew in #656 (`#host-desktops` 4/8) while already over the 500-line budget

**Category:** Deferred decomposition (developer consent given at `/pr-wrap` step 3.5, 2026-10-20)
**Source:** [PR #656](https://github.com/uppin/tddy-coder/pull/656) — Hosts screen tooling mount and the daemon-side tooling cache

| File | Production lines | Record |
|---|---|---|
| `tddy-host-service/src/service.rs` | 936 → 961 | `oversized-file-service.md` |

Growth is 25 lines, all of it the cache wiring in `get_host_tooling` (a TTL constant, a `HostServiceImpl`
field and the keyed `get_or_probe` call). The cache itself is a new module, `host_tooling_cache.rs`
(115 production lines), so the growth is only the call site.

The file needs the four-seam split recorded in the code issue (peer routing, host keys and SSH,
host stats, prompts). That is a restructure pass of its own, not something a 25-line feature PR
should carry. Do it as one pass after the `#host-desktops` stack lands, together with the other
deferrals that name this file.

**Correction (2026-10-21, `#host-desktops` sweep planning):** the file went 936 → **972** in #656 (+36:
48 lines added, 12 removed, measured from `300c3da4c^` → `300c3da4c`), not 936 → 961. The record carries
the corrected row. The sweep node (`#host-desktops` 9/9) plans the split; see its changeset.
