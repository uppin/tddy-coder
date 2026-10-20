# 2026-10-20 — Screen sharing on the Hosts screen (`#host-desktops` 4/8)

**Type:** Feature

Packages: `tddy-web`, `tddy-host-service`. The Hosts screen mounts the tooling section on online rows, which makes the host desktop feature and its Connect action reachable in the running app; the daemon caches and de-duplicates the tooling probe so that mount does not become a per-visit subprocess fan-out. See the package entries: [tddy-host-service](../../packages/tddy-host-service/docs/changesets/2026-10-20-host-desktops-hosts-mount.md), [tddy-web](../../packages/tddy-web/docs/changesets/2026-10-20-host-desktops-hosts-mount.md).

**Backlog entries closed:** *No code path issues `GetHostTooling` yet* and *The tooling probe has no cache and no in-flight dedup* (both 2026-09-06).

**Code issues, final measurements:**

| Record | Before → after (production lines) | Outcome |
|---|---|---|
| `oversized-file-service.md` (`tddy-host-service/src/service.rs`) | 936 → 961 | regressed by 25 (the cache wiring); split deferred with developer consent 2026-10-20; record kept |
| `oversized-file-host-tooling.md` (`host_tooling.rs`) | 775 → 775 | unchanged — the cache lives in a new module; record kept |

New module `host_tooling_cache.rs` is 115+ production lines; `HostRowToolingCell.tsx` and `useHostTooling.ts` are 66 and 48 lines.
