//! Session id resolution shared by sandbox and remote MCP tool dispatch. The wire protocol
//! between in-jail `tddy-sandbox-runner` and `tddy-tools --mcp` (previously defined here as
//! `ToolIpcRequest`/`ToolIpcResponse`, raw JSON over a Unix socket) now speaks `tddy-rpc`'s
//! length-prefixed framing instead (see `session_tool_client::dispatch_via_sandbox_ipc` and
//! `tddy-sandbox-runner`'s `ToolExecService`) — those types were removed as dead code once their
//! only production consumers were migrated.

/// Session id for sandbox or remote MCP tool dispatch.
pub fn session_id_from_env() -> String {
    std::env::var("TDDY_SANDBOX_SESSION_ID")
        .or_else(|_| std::env::var("TDDY_REMOTE_SESSION_ID"))
        .unwrap_or_default()
}
