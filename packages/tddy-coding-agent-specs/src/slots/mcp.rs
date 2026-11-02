//! The `mcp` slot: the tool server registration file and the flag that loads it.

use std::path::{Path, PathBuf};

use super::McpServerCommand;
use crate::{McpFormat, McpSlot};

/// The registration file's name, as `write_claude_mcp_config` names it.
const MCP_CONFIG_FILENAME: &str = "claude-mcp-config.json";

/// The registration file for `server` in `slot`'s format, under `scratch_dir`, and the argv that
/// passes it (`--mcp-config <path>`).
pub fn render_mcp(
    slot: &McpSlot,
    server: &McpServerCommand,
    scratch_dir: &Path,
) -> ((PathBuf, Vec<u8>), Vec<String>) {
    let McpSlot::File { flag, format } = slot;
    let McpFormat::ClaudeMcpJson = format;
    let mut entry = serde_json::json!({
        "command": server.command.to_string_lossy(),
        "args": server.args,
    });
    if !server.env.is_empty() {
        entry["env"] = serde_json::json!(server.env);
    }
    let config = serde_json::json!({ "mcpServers": { server.name.as_str(): entry } });
    let path = scratch_dir.join(MCP_CONFIG_FILENAME);
    let argv = vec![flag.clone(), path.to_string_lossy().into_owned()];
    ((path, config.to_string().into_bytes()), argv)
}
