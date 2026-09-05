# 2026-07-01 — Sandbox tool-IPC migrated to `tddy-rpc` framing

**Type:** Feature

`dispatch_via_sandbox_ipc` now connects the `TDDY_SANDBOX_TOOL_IPC` `UnixStream` and wraps it via `tddy_stdio::StdioEndpoint::from_duplex`, delegating to a new `dispatch_via_stdio_rpc`, replacing the old unframed single-`read()`/`write_all()` JSON protocol that could silently truncate large payloads — same socket path, no topology change. `tddy-sandbox-runner`'s `start_tool_ipc_server` updated symmetrically. Proven through a real Seatbelt jail (`sandbox_runner_session_channel_tool_exec_round_trips`, unchanged). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#253](https://github.com/uppin/tddy-coder/pull/253). (tddy-tools, tddy-sandbox-runner, tddy-rpc, tddy-stdio)
