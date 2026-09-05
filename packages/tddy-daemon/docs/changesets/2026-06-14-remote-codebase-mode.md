# 2026-06-14 — Remote-codebase mode

**Type:** Feature

workspace sessions (`start_workspace_session`, `resolve_worktree_root_for_session`); `tool_engine.rs` (10 cursor tools, `contain_path` security); `tool_catalog.rs`; `shell_job_registry.rs` (background shell + Await); `execute_tool`/`list_exec_tools` handlers; relay mode (`--relay`, `startup_config_check`, `RelayConfig`); `IdleTimeoutTracker`, `with_idle_tracker`, `record_rpc_activity`; `forward_to_peer` + per-peer `RpcClient` cache; `classify_peer_route`; external shutdown channel in `run_server`; idle monitor task in `main.rs`. Feature [remote-codebase-mode.md](../../../../docs/ft/daemon/remote-codebase-mode.md); product [daemon/changelog/](../../../../docs/ft/daemon/changelog/). (tddy-daemon)
