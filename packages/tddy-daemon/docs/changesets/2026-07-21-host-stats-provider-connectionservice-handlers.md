# 2026-07-21 — Host stats provider + `ConnectionService` handlers

**Type:** Feature

new `host_stats.rs`: `sysinfo`-backed `SysinfoHostStats` (long-lived per-core CPU sampler; `select_mount_for_path` longest-path-component-prefix disk resolution against `$HOME/<repos_base_path_or_default>`, largest-mount fallback) behind a `HostStats` trait. `ConnectionServiceImpl` gains an `Arc<dyn HostStats>` field (default `SysinfoHostStats`) + a `with_host_stats` test seam, and token-validating `get_host_cpu_stats`/`get_host_disk_stats` handlers (tonic adapter delegates both). Units: 4 `select_mount_for_path` + 4 handler token/mapping tests. Doc [connection-service.md § Host stats](../connection-service.md#host-stats-host-stats-footer). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#306](https://github.com/uppin/tddy-coder/pull/306). (tddy-daemon)
