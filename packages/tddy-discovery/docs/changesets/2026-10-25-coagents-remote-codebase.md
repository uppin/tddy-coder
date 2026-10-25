# 2026-10-25 — The roster stream and conversation link ride `DaemonHttp`

**Type:** Feature

`decide_roster_subscription` and `connect_facilitating_daemon` accept `SessionToolTransport::DaemonHttp`,
using `tddy_session_tool_client::daemon_http_rpc::DaemonHttpRpcClient` with the transport's envelope; the
refusal and its `TODO(session-agent-roster)` are gone. `roster/stream.rs` (545 production lines) is
recorded in `docs/code-issues/oversized-file-roster-stream.md`. `#coagents` 6/8,
[#725](https://github.com/uppin/tddy-coder/pull/725); see [remote-codebase mode](../../../../docs/ft/daemon/remote-codebase-mode.md#coding-subagents-on-the-codebase-host).
