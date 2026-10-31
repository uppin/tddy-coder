//! The `mcp` slot: the tool server registration file and the flag that loads it.

use std::path::{Path, PathBuf};

use super::McpServerCommand;
use crate::McpSlot;

/// The registration file for `server` in `slot`'s format, under `scratch_dir`, and the argv that
/// passes it (`--mcp-config <path>`).
pub fn render_mcp(
    slot: &McpSlot,
    server: &McpServerCommand,
    scratch_dir: &Path,
) -> ((PathBuf, Vec<u8>), Vec<String>) {
    // TODO(claude-host): implement — `claude-mcp-json` is `claude-mcp-config.json` holding
    // `{"mcpServers": {name: {command, args, env?}}}`, as `write_claude_mcp_config` writes today.
    let _ = (slot, server, scratch_dir);
    todo!("claude-host: render_mcp")
}
