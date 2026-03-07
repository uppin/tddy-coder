//! tddy-permission: MCP server for Claude Code permission prompts.
//!
//! Implements the approval_prompt tool used by --permission-prompt-tool.
//! When non-TTY: denies unexpected requests. When TTY: forwards to tddy-coder via IPC.

mod server;

use anyhow::Result;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use server::PermissionServer;

#[tokio::main]
async fn main() -> Result<()> {
    let service = PermissionServer::new();
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
