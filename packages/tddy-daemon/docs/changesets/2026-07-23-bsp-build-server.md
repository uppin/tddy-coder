# 2026-07-23 — **bsp-build-server

**Type:** Feature

session-addressed `DaemonBspService`** — new `bsp_service.rs`: `DaemonBspService` (`SessionPathsResolver` + `tddy_data_dir`) implements the `bsp.BspService` trait by resolving each request's `(session_token, session_id)` to the session's `(session_dir, repo_root)` — token → os_user → sessions_base → `.session.yaml` `repo_path`, reusing the `ExecuteTool` preamble — and delegating to a per-request `tddy_bsp::BspServiceImpl`. `main.rs` repoints catalog-provider registration to `tddy_bsp::register_catalog_provider()`, builds the session-paths resolver, and registers a `bsp.BspService` entry in `rpc_entries`; `Cargo.toml` adds the `tddy-bsp` dep. Tests: bsp_service 2. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). Feature [bsp-build-server.md](../../../../docs/ft/coder/bsp-build-server.md). (tddy-daemon)
