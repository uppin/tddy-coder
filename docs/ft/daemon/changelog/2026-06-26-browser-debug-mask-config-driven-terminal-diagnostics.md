# 2026-06-26 — Browser DEBUG mask — config-driven terminal diagnostics

- `DaemonConfig.debug: Option<String>` threaded through `run_server` → `ClientConfig.debug` and served at `GET /api/config`; browser picks up the mask for scoped `[tddy]` console logging
- `dev.daemon.yaml` ships `debug: "tddy:term:*"` — covers all terminal namespaces; comment out or set `""` to disable
