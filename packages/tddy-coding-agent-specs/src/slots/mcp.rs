//! The `mcp` slot: the tool server registration file and the flag that loads it, or the path a
//! placed registration is written to.

use std::path::{Path, PathBuf};

use super::cursor::{placed_path, render_cursor_mcp_json};
use super::headless_mcp;
use super::{McpServerCommand, SlotError};
use crate::{McpFormat, McpSlot};

/// The registration file's name, as `write_claude_mcp_config` names it.
pub(crate) const MCP_CONFIG_FILENAME: &str = "claude-mcp-config.json";

/// A registration file: the path it is written to, and its bytes.
pub type RegistrationFile = (PathBuf, Vec<u8>);

/// The registration file for `server` in `slot`, and the argv that passes it. A file slot writes
/// it under `scratch_dir` and passes `--mcp-config <path>`, then the slot's `strict_flag` when it
/// declares one. A placed slot writes it where the agent reads it — its `{home}` resolved against
/// `home` — and passes no argv. The headless kinds render through [`headless_mcp`]; an inline slot
/// writes no file, and a workdir slot's is under `workdir`.
pub fn render_mcp(
    slot: &McpSlot,
    server: &McpServerCommand,
    scratch_dir: &Path,
    workdir: &Path,
    home: Option<&Path>,
) -> Result<(Option<RegistrationFile>, Vec<String>), SlotError> {
    match slot {
        McpSlot::File {
            flag,
            format,
            strict_flag,
        } => {
            let path = scratch_dir.join(MCP_CONFIG_FILENAME);
            let mut argv = vec![flag.clone(), path.to_string_lossy().into_owned()];
            argv.extend(strict_flag.iter().cloned());
            Ok((Some(registration(format, server, path)), argv))
        }
        McpSlot::Placed { path, format } => {
            let path = placed_path(path, home)?;
            Ok((Some(registration(format, server, path)), Vec::new()))
        }
        // `slots::render` refuses an unsupported slot given a tool server before it gets here.
        McpSlot::Unsupported => {
            unreachable!("an unsupported mcp slot registers no server: slots::render refuses it")
        }
        McpSlot::Inline { flag, key_prefix } => {
            let mut argv = headless_mcp::render_inline_mcp(flag, key_prefix, server);
            if let Some(cwd) = &server.cwd {
                argv.extend(headless_mcp::render_inline_mcp_cwd(
                    flag, key_prefix, server, cwd,
                ));
            }
            Ok((None, argv))
        }
        McpSlot::Argv { flag } => {
            let (file, argv) = headless_mcp::render_argv_mcp(flag, server, scratch_dir);
            Ok((Some(file), argv))
        }
        // The server travels in `session/new` `mcp_servers`, which `slots::acp::acp_mcp_servers`
        // renders: no file and no argv entry.
        McpSlot::AcpSession => Ok((None, Vec::new())),
        McpSlot::Workdir { path, format } => Ok((
            Some(headless_mcp::render_workdir_mcp(
                path, *format, server, workdir,
            )),
            Vec::new(),
        )),
    }
}

/// `server` registered in `format`, written to `path`.
fn registration(format: &McpFormat, server: &McpServerCommand, path: PathBuf) -> RegistrationFile {
    match format {
        McpFormat::ClaudeMcpJson => (path, mcp_servers_json(server).to_string().into_bytes()),
        McpFormat::CursorMcpJson => render_cursor_mcp_json(server, &path),
    }
}

/// `server` as an `mcpServers` registration: `{"mcpServers": {name: {command, args, env?}}}`, its
/// `env` omitted when empty. The shape both formats write.
pub(crate) fn mcp_servers_json(server: &McpServerCommand) -> serde_json::Value {
    let mut entry = serde_json::json!({
        "command": server.command.to_string_lossy(),
        "args": server.args,
    });
    if !server.env.is_empty() {
        entry["env"] = serde_json::json!(server.env);
    }
    serde_json::json!({ "mcpServers": { server.name.as_str(): entry } })
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
            cwd: None,
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
            Path::new("/work"),
            None,
        )
        .unwrap();

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
            Path::new("/work"),
            None,
        )
        .unwrap();

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
