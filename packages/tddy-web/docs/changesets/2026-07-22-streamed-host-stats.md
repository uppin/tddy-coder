# 2026-07-22 — **Streamed host stats

**Type:** Refactor

one `StreamHostStats` subscription replaces the two poll hooks** — `useHostCpuStats`/`useHostDiskStats` (interval polling `GetHostCpuStats`/`GetHostDiskStats`) are replaced by a single `useHostStats(): { perCorePercent, disk }` that subscribes once to `client.streamHostStats(...)` via a `for await` loop over `useDaemonClient(ConnectionService)`, applying each server-pushed `HostStatsEvent` (cleanup mirrors `useSessionActivity`: a `cancelled` flag, AbortError swallowed). `HostStatsFooter` calls the single hook; `CPU_REFRESH_MS`/`DISK_REFRESH_MS` removed; `connectionServiceBackend` now serves `streamHostStats` (async generator, `hostStatsStreamCount`) instead of the two unary stubs. Cypress `HostStatsFooterAcceptance` extended to 6 (single-subscription + live-update). Feature [host-stats-footer.md](../../../../docs/ft/web/host-stats-footer.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
