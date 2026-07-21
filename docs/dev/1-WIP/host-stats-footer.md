# Changeset: host-stats-footer — relocate byte-traffic to a bottom footer; add disk + per-core CPU host stats

**Date:** 2026-07-21  
**Branch:** `feat-info-fixes`  
**Packages:** `tddy-service` (proto), `tddy-daemon` (host-stats gathering + RPC), `tddy-web` (footer UI + polling)  
**Feature PRD:** [docs/ft/web/host-stats-footer.md](../../ft/web/host-stats-footer.md) (also amends [session-drawer.md § Session Traffic Strip](../../ft/web/session-drawer.md#session-traffic-strip))

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Add `GetHostCpuStats` / `GetHostDiskStats` to `connection.proto` (+ request/response messages)
- [ ] Regenerate TS bindings (`bun run generate`); Rust bindings regenerate via `tddy-service/build.rs`
- [x] Add `sysinfo` workspace dependency (user-approved)
- [x] Implement `host_stats.rs` in `tddy-daemon`: `MountUsage`, `select_mount_for_path`, `HostStats` trait + `SysinfoHostStats`
- [x] Wire `Arc<dyn HostStats>` into `ConnectionServiceImpl` (`new` default = `SysinfoHostStats`; `with_host_stats` test seam)
- [x] Implement `get_host_cpu_stats` / `get_host_disk_stats` handlers (token validation + map provider → proto)
- [x] Add `useHostCpuStats` (5 s) / `useHostDiskStats` (60 s) polling hooks (`tddy-web/src/rpc/useHostStats.ts`)
- [x] Implement `DiskSpaceIndicator`, `CpuCoresIndicator`, `HostStatsFooter` (`tddy-web/src/components/sessions/`)
- [x] Relocate `SessionTrafficStrip`/`StatusBar` into `HostStatsFooter`; remove traffic from `SessionsDrawerScreen` top header
- [x] Mount `HostStatsFooter` at the bottom of `SessionsDrawerScreen`
- [x] Add testids (`host-stats-footer`, `disk-space-available`, `cpu-cores`, `cpu-core-bar`)

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/HostStatsFooterAcceptance.cy.tsx`

## Unit / integration tests

- [x] `packages/tddy-daemon/src/host_stats.rs` (`#[cfg(test)]` — `select_mount_for_path` cases)
- [x] `packages/tddy-daemon/src/connection_service.rs` (`#[cfg(test)]` — `get_host_cpu_stats` / `get_host_disk_stats` handler token-validation + mapping cases)
- [x] `packages/tddy-web/src/components/sessions/hostStatsFormat.test.ts` (disk-free formatting + CPU clamp helper)

## Validation Results

### validate-changes (2026-07-21)

**Critical: 0 · Warning: 0 · Info: 2** — no fixes required.

- **[INFO]** `host_stats.rs::disk_for_project_dir` falls back to the largest mount by capacity when no mount is a component-prefix of the project dir (root `/` normally matches). Documented + `TODO(host-stats-footer)`.
- **[INFO]** `resolve_default_project_dir` derives `$HOME/<repos_base_path_or_default>` because `DaemonConfig` exposes no explicit project-dir override. `TODO(host-stats-footer)` to prefer one if added.

Verified: both new handlers validate `session_token` (→ `Unauthenticated`) and call `record_rpc_activity()`; no production `unwrap`/`expect` beyond a standard mutex guard; frontend catches log via `console.debug` (matches existing pattern); no hardcoded secrets, test-only branches, or unconsented failure-hiding fallbacks.

### validate-tests (2026-07-21)

**18 tests across 4 files · 0 issues.** All fluent-compliant: Given/When/Then, page-object helpers (no raw selectors in bodies), one behavior per test, `mountWithRpc` + in-memory backend (no `cy.intercept`), builders (`a_mount_at`, `FakeHostStats`), exact assertions, and edge/error cases (invalid-token rejection, longest-prefix vs partial-component, boundary percentages, zero/bigint disk).

### validate-prod-ready (2026-07-21)

**8 files · Blockers: 0 · Warnings: 1.** No mock/fake in production (`FakeHostStats` is `#[cfg(test)]`), no unused code, no `dbg!`/`println!`/`eprintln!`; hook `console.debug` is browser-side, not a TUI path. **[WARNING]** disk largest-mount fallback (display-only edge case, `TODO`-flagged, acknowledged). Two intentional `TODO(host-stats-footer)` markers.

### analyze-clean-code (2026-07-21)

**Score: A.** No must-refactor / needs-attention items. Named refresh-interval constants; no production magic values. Optional (not applied): the two polling hooks share a fetch/interval/cleanup skeleton, but bespoke polling hooks are the repo's established idiom (`useLiveKitPing`, `useMeterSnapshot`) — kept consistent rather than over-abstracted.

## Delta summary

### `tddy-service` (proto)

**Modified:** `proto/connection.proto` — two new unary `ConnectionService` methods and their
messages:

```proto
rpc GetHostCpuStats(GetHostCpuStatsRequest) returns (GetHostCpuStatsResponse);
rpc GetHostDiskStats(GetHostDiskStatsRequest) returns (GetHostDiskStatsResponse);

message GetHostCpuStatsRequest { string session_token = 1; }
message GetHostCpuStatsResponse { repeated float per_core_percent = 1; }

message GetHostDiskStatsRequest { string session_token = 1; }
message GetHostDiskStatsResponse {
  uint64 available_bytes = 1;
  uint64 total_bytes = 2;
  string project_dir = 3;
}
```

### `tddy-daemon`

**New file:** `src/host_stats.rs`
- `MountUsage { mount_point: PathBuf, available_bytes: u64, total_bytes: u64 }`.
- `select_mount_for_path(mounts, target) -> Option<&MountUsage>` — pure: picks the mount whose
  `mount_point` is the **longest path-component prefix** of `target` (so `/home` beats `/` for
  `/home/dev/repos`, and `/ho` never matches `/home`). Returns `None` when no mount is a prefix.
- `HostStats` trait (`cpu_per_core_percent`, `disk_for_project_dir`) + `SysinfoHostStats`
  implementation backed by the `sysinfo` crate; disk target resolved from `DaemonConfig` as
  `$HOME/<repos_base_path_or_default>` (`DaemonConfig` exposes no explicit project-dir/`base_path`
  override today — a `TODO(host-stats-footer)` in `resolve_default_project_dir` notes to prefer one
  if added).

**Modified:** `src/connection_service.rs`
- `ConnectionServiceImpl` gains an `Arc<dyn HostStats>` field (default `SysinfoHostStats`) and a
  `with_host_stats(...)` builder seam for tests.
- `get_host_cpu_stats` / `get_host_disk_stats` handlers: validate `session_token`, then map the
  provider output into the proto responses.

**Dependency:** add `sysinfo` to the workspace (per-core CPU usage + disk enumeration).

### `tddy-web`

**New files:**
- `src/rpc/useHostStats.ts` — `useHostCpuStats()` (polls `GetHostCpuStats` every 5 s) and
  `useHostDiskStats()` (polls `GetHostDiskStats` every 60 s) over `useDaemonClient(ConnectionService)`.
- `src/components/sessions/hostStatsFormat.ts` — `formatDiskFree(availableBytes)` and
  `clampCorePercent(raw)` (clamp to 0–100).
- `src/components/sessions/DiskSpaceIndicator.tsx` — presentational free-space readout.
- `src/components/sessions/CpuCoresIndicator.tsx` — presentational per-core mini-bar row.
- `src/components/sessions/HostStatsFooter.tsx` — the bottom footer combining the relocated
  traffic readout with the disk and CPU indicators.

**Modified files:**
- `src/components/sessions/SessionsDrawerScreen.tsx` — remove `StatusBar` from the top header;
  mount `HostStatsFooter` at the bottom of the screen.
- `src/components/sessions/SessionTrafficStrip.tsx` — footer placement (border/`flex-shrink-0`
  adjustment for a bottom strip).
- `cypress/support/rpc/connectionServiceBackend.ts` — stub `getHostCpuStats` / `getHostDiskStats`
  with configurable fixture values.
- `cypress/support/testIds.ts` — new footer testids.
- `src/components/sessions/StatusBar.tsx` — moved under / consumed by the footer.
