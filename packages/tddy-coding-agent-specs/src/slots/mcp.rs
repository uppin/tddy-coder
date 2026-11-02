//! The `mcp` slot: the tool server registration file and the flag that loads it.

use std::path::{Path, PathBuf};

use super::McpServerCommand;
use crate::{McpFormat, McpSlot};

/// The registration file's name, as `write_claude_mcp_config` names it.
const MCP_CONFIG_FILENAME: &str = "claude-mcp-config.json";

/// The registration file for `server` in `slot`'s format, under `scratch_dir`, and the argv that
/// passes it (`--mcp-config <path>`, then the slot's `strict_flag` when it declares one).
pub fn render_mcp(
    slot: &McpSlot,
    server: &McpServerCommand,
    scratch_dir: &Path,
) -> ((PathBuf, Vec<u8>), Vec<String>) {
    let (flag, format, strict_flag) = match slot {
        McpSlot::File {
            flag,
            format,
            strict_flag,
        } => (flag, format, strict_flag),
        // TODO(cursor-cli): a placed registration is rendered by `cursor::render_cursor_mcp_json`.
        McpSlot::Placed { .. } => todo!("cursor-cli: McpSlot::Placed in render_mcp"),
    };
    #[allow(unreachable_patterns)]
    match format {
        McpFormat::ClaudeMcpJson => {}
        // TODO(cursor-cli): a file slot in another format.
        _ => todo!("cursor-cli: McpFormat other than ClaudeMcpJson in render_mcp"),
    }
    let mut entry = serde_json::json!({
        "command": server.command.to_string_lossy(),
        "args": server.args,
    });
    if !server.env.is_empty() {
        entry["env"] = serde_json::json!(server.env);
    }
    let config = serde_json::json!({ "mcpServers": { server.name.as_str(): entry } });
    let path = scratch_dir.join(MCP_CONFIG_FILENAME);
    let mut argv = vec![flag.clone(), path.to_string_lossy().into_owned()];
    argv.extend(strict_flag.iter().cloned());
    ((path, config.to_string().into_bytes()), argv)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;

    use super::*;

    fn a_file_slot(strict_flag: Option<&str>) -> McpSlot {
        McpSlot::File {
            flag: "--mcp-config".to_string(),
            format: McpFormat::ClaudeMcpJson,
            strict_flag: strict_flag.map(str::to_string),
        }
    }

    fn the_tddy_tools_server(env: BTreeMap<String, String>) -> McpServerCommand {
        McpServerCommand {
            name: "tddy-tools".to_string(),
            command: PathBuf::from("/opt/tddy/bin/tddy-tools"),
            args: vec!["--mcp".to_string()],
            env,
        }
    }

    #[test]
    fn a_slot_with_a_strict_flag_passes_it_after_the_registration_path() {
        // Given
        let scratch = Path::new("/sessions/0190-a");

        // When
        let (_, argv) = render_mcp(
            &a_file_slot(Some("--strict-mcp-config")),
            &the_tddy_tools_server(BTreeMap::new()),
            scratch,
        );

        // Then
        assert_eq!(
            argv,
            vec![
                "--mcp-config".to_string(),
                "/sessions/0190-a/claude-mcp-config.json".to_string(),
                "--strict-mcp-config".to_string(),
            ]
        );
    }

    #[test]
    fn a_slot_without_a_strict_flag_passes_only_the_registration_path() {
        // Given
        let scratch = Path::new("/sessions/0190-a");

        // When
        let (_, argv) = render_mcp(
            &a_file_slot(None),
            &the_tddy_tools_server(BTreeMap::new()),
            scratch,
        );

        // Then
        assert_eq!(
            argv,
            vec![
                "--mcp-config".to_string(),
                "/sessions/0190-a/claude-mcp-config.json".to_string(),
            ]
        );
    }
}
