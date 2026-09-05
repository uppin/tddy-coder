# 2026-07-01 — `SandboxHandle::take_stdio()`; `ToolIpcRequest`/`ToolIpcResponse` removed

**Type:** Feature

new `SandboxHandle::take_stdio()` exposes a jail-spawned process's piped (blocking) stdin/stdout for `--stdio` bridging (see `tddy_daemon::sandbox_session::bridge_sandbox_stdio`). `tool_ipc.rs`'s `ToolIpcRequest`/`ToolIpcResponse` (the old unframed JSON tool-IPC protocol) deleted as dead code once migrated onto `tddy-rpc` framing — `session_id_from_env` remains. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#253](https://github.com/uppin/tddy-coder/pull/253). (tddy-sandbox, tddy-sandbox-darwin, tddy-daemon, tddy-tools)
