# 2026-07-22 — Host stats RPC unified into `StreamHostStats`

**Type:** Refactor

`proto/connection.proto` removes the unary `GetHostCpuStats`/`GetHostDiskStats` methods and their four messages, and adds a **server-streaming** `StreamHostStats(StreamHostStatsRequest) returns (stream HostStatsEvent)` with `HostStatsEvent { HostCpuStats cpu; HostDiskStats disk }` (both always populated), `HostCpuStats { per_core_percent: repeated float }`, and `HostDiskStats { available_bytes, total_bytes, project_dir }`. Regenerated Rust + `tddy-web/src/gen/connection_pb.ts`. Feature [host-stats-footer.md](../../../../docs/ft/web/host-stats-footer.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
