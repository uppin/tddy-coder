# 2026-07-21 — **Host stats RPCs

**Type:** Feature

`GetHostCpuStats` / `GetHostDiskStats` on `ConnectionService`** — `proto/connection.proto` adds two session-token-validated unary methods: per-core CPU (`per_core_percent: repeated float`, 0..100) and project-dir disk (`available_bytes`/`total_bytes` uint64 + `project_dir`) for the web's Host Stats Footer. Feature [host-stats-footer.md](../../../../docs/ft/web/host-stats-footer.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#306](https://github.com/uppin/tddy-coder/pull/306). (tddy-service)
