# 2026-06-14 — Remote-codebase mode

**Type:** Feature

`relay.rs`: `ensure_relay_daemon` + `RelayEndpoint` (lazy spawn, TCP health-check, discovery file); `server.rs`: `dispatch_dynamic_tool` real HTTP POST to relay `ExecuteTool`; `remote_cli.rs`: `list-tools` via `ListExecTools` Connect POST, `start-session`/`connect-session`/`sync-context` implemented with `connect_post` + `resolve_base_url`. Feature [remote-codebase-mode.md](../../../../docs/ft/daemon/remote-codebase-mode.md). (tddy-tools)
