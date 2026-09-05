# 2026-07-02 — Sandboxed-session lifecycle switched to stdio

**Type:** Feature

`connection_service.rs`'s spawn/dial orchestration and `sandbox_session::dial_and_bridge` now dial `tddy-sandbox-runner` exclusively over its stdio-served `SessionChannel` (`bridge_sandbox_stdio` → `StdioSandboxClient` → `run_host_relay`), wiring in the primitive the previous changeset proved but didn't use; `--grpc-socket`/`--grpc-listen-port`/`--grpc-uds` argv and `pick_free_loopback_port`'s control-port allocation deleted for this call site (no dual-path fallback); `connect_sandbox_client`/`connect_sandbox_session_client` are kept (still used by `sandbox_action.rs`'s separate generic-action-execution flow). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon, tddy-sandbox-runner)
