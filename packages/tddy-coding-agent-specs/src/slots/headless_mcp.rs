//! The MCP slot kinds the headless agents need (`#agent-tools 9/12`): Codex's inline `-c`
//! overrides, Oz's `--mcp <path>` file, and a headless Cursor's worktree `.cursor/mcp.json` — and
//! the paths each kind writes, so a launch can record them with its conversation.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-headless-cursor-codex-oz-from-specs.md`.
//!
//! Each renderer reproduces what a hand-built path writes today: `headless_driver/argv.rs`'s
//! `codex_mcp_server_overrides`, and `coding_agent_launcher.rs`'s `mcp_servers_by_name` (Oz) and
//! `write_cursor_project_mcp_config` (Cursor).

use std::path::{Path, PathBuf};

use super::{McpServerCommand, SlotError};
use crate::{McpFormat, McpSlot};

/// The file an [`McpSlot::Argv`] registration is written as, under the launch's scratch directory
/// — the name the hand-built Oz launch gives it today.
pub const ARGV_MCP_FILE: &str = "mcp.json";

/// `server` as `flag <key_prefix>.<name>.<field>=<value>` pairs for `command`, `args` and
/// `env_vars`, in that order. Values are TOML (JSON's spelling of a string and a list of strings,
/// which TOML reads the same); `env_vars` lists the environment's **names** only, never a value.
pub fn render_inline_mcp(flag: &str, key_prefix: &str, server: &McpServerCommand) -> Vec<String> {
    // TODO(headless-others): implement — `argv::codex_mcp_server_overrides` moves here; green
    // deletes it with `headless_driver/argv.rs`.
    let _ = (flag, key_prefix, server);
    todo!("headless-others: render_inline_mcp")
}

/// `server` written to `<scratch_dir>/`[`ARGV_MCP_FILE`] in the servers-by-name shape
/// `{"<name>": {"command", "args", "env"?}}` (compact, `env` omitted when empty, as every other
/// registration renders it), and the argv that passes it: `flag <path>`.
pub fn render_argv_mcp(
    flag: &str,
    server: &McpServerCommand,
    scratch_dir: &Path,
) -> ((PathBuf, Vec<u8>), Vec<String>) {
    // TODO(headless-others): implement — the bytes `mcp_servers_by_name` writes today.
    let _ = (flag, server, scratch_dir);
    todo!("headless-others: render_argv_mcp")
}

/// `server` written to `<workdir>/<path>` in `format`. A `cursor-mcp-json` registration is the
/// `{"mcpServers": {name: {command, args, env?}}}` the hand-built headless Cursor launch and
/// `CursorBackend` write into the worktree today (compact, `env` omitted when empty). No flag
/// passes it.
pub fn render_workdir_mcp(
    path: &Path,
    format: McpFormat,
    server: &McpServerCommand,
    workdir: &Path,
) -> (PathBuf, Vec<u8>) {
    // TODO(headless-others): implement.
    let _ = (path, format, server, workdir);
    todo!("headless-others: render_workdir_mcp")
}

/// The files `slot` writes for a launch on `workdir` with `scratch_dir` and `home`, without
/// rendering them: what a launcher records with the conversation before it spawns anything.
///
/// - [`McpSlot::File`] — the format's file under `scratch_dir` (`claude-mcp-config.json`);
/// - [`McpSlot::Placed`] — its placed path (`{home}` bound to `home`, or [`SlotError::UnboundHome`]);
/// - [`McpSlot::Argv`] — `<scratch_dir>/`[`ARGV_MCP_FILE`];
/// - [`McpSlot::Workdir`] — `<workdir>/<path>`;
/// - [`McpSlot::Inline`] and [`McpSlot::Unsupported`] — none.
pub fn mcp_written_paths(
    slot: &McpSlot,
    workdir: &Path,
    scratch_dir: &Path,
    home: Option<&Path>,
) -> Result<Vec<PathBuf>, SlotError> {
    // TODO(headless-others): implement.
    let _ = (slot, workdir, scratch_dir, home);
    todo!("headless-others: mcp_written_paths")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;

    use super::*;

    const TDDY_TOOLS: &str = "/opt/tddy/bin/tddy-tools";
    const TOKEN: &str = "minted-agent-token";

    /// The `tddy-tools` server a launcher hands a headless conversation, its credential in its
    /// environment.
    fn the_tool_server() -> McpServerCommand {
        McpServerCommand {
            name: "tddy-tools".to_string(),
            command: PathBuf::from(TDDY_TOOLS),
            args: vec!["--mcp".to_string()],
            env: BTreeMap::from([
                ("TDDY_CONVERSATION_ID".to_string(), "conv-1".to_string()),
                ("TDDY_REMOTE_SESSION_TOKEN".to_string(), TOKEN.to_string()),
            ]),
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_inline_slot_renders_command_args_and_env_var_names_as_toml_overrides() {
        // When
        let argv = render_inline_mcp("-c", "mcp_servers", &the_tool_server());

        // Then
        assert_eq!(
            argv,
            strings(&[
                "-c",
                "mcp_servers.tddy-tools.command=\"/opt/tddy/bin/tddy-tools\"",
                "-c",
                "mcp_servers.tddy-tools.args=[\"--mcp\"]",
                "-c",
                "mcp_servers.tddy-tools.env_vars=[\"TDDY_CONVERSATION_ID\",\"TDDY_REMOTE_SESSION_TOKEN\"]",
            ])
        );
    }

    #[test]
    fn an_inline_slot_escapes_a_quote_and_a_backslash_in_the_command_as_toml_does() {
        // Given
        let server = McpServerCommand {
            command: PathBuf::from(r#"/opt/a "b"\c/tddy-tools"#),
            env: BTreeMap::new(),
            ..the_tool_server()
        };

        // When
        let argv = render_inline_mcp("-c", "mcp_servers", &server);

        // Then
        assert_eq!(
            argv[1],
            r#"mcp_servers.tddy-tools.command="/opt/a \"b\"\\c/tddy-tools""#
        );
    }

    #[test]
    fn an_inline_slot_never_carries_the_servers_credential_on_the_argv() {
        // Given — the positive control: the credential is in the server's environment
        let server = the_tool_server();
        assert_eq!(
            server
                .env
                .get("TDDY_REMOTE_SESSION_TOKEN")
                .map(String::as_str),
            Some(TOKEN)
        );

        // When
        let argv = render_inline_mcp("-c", "mcp_servers", &server);

        // Then
        assert_eq!(
            argv.iter().filter(|arg| arg.contains(TOKEN)).count(),
            0,
            "another user's `ps` reads the argv: {argv:?}"
        );
    }

    #[test]
    fn an_argv_slot_writes_the_servers_by_name_file_under_scratch_and_passes_it_by_its_flag() {
        // Given
        let scratch = Path::new("/data/headless/conv-1");

        // When
        let rendered = render_argv_mcp("--mcp", &the_tool_server(), scratch);

        // Then
        let path = scratch.join(ARGV_MCP_FILE);
        let expected = serde_json::json!({
            "tddy-tools": {
                "command": TDDY_TOOLS,
                "args": ["--mcp"],
                "env": {
                    "TDDY_CONVERSATION_ID": "conv-1",
                    "TDDY_REMOTE_SESSION_TOKEN": TOKEN,
                },
            }
        });
        assert_eq!(
            rendered,
            (
                (path.clone(), expected.to_string().into_bytes()),
                vec!["--mcp".to_string(), path.to_string_lossy().into_owned()],
            )
        );
    }

    #[test]
    fn a_workdir_slot_writes_the_cursor_mcp_json_into_the_working_directory() {
        // Given
        let workdir = Path::new("/home/dev/worktrees/parser-fix");

        // When
        let rendered = render_workdir_mcp(
            Path::new(".cursor/mcp.json"),
            McpFormat::CursorMcpJson,
            &the_tool_server(),
            workdir,
        );

        // Then
        let expected = serde_json::json!({
            "mcpServers": {
                "tddy-tools": {
                    "command": TDDY_TOOLS,
                    "args": ["--mcp"],
                    "env": {
                        "TDDY_CONVERSATION_ID": "conv-1",
                        "TDDY_REMOTE_SESSION_TOKEN": TOKEN,
                    },
                }
            }
        });
        assert_eq!(
            rendered,
            (
                workdir.join(".cursor/mcp.json"),
                expected.to_string().into_bytes()
            )
        );
    }

    #[test]
    fn every_mcp_kind_names_the_files_it_writes_and_inline_names_none() {
        // Given
        let workdir = Path::new("/home/dev/repo");
        let scratch = Path::new("/data/headless/conv-1");
        let home = Path::new("/home/dev");
        let slots = [
            McpSlot::File {
                flag: "--mcp-config".to_string(),
                format: McpFormat::ClaudeMcpJson,
            },
            McpSlot::Placed {
                path: "{home}/.cursor/mcp.json".to_string(),
                format: McpFormat::CursorMcpJson,
            },
            McpSlot::Argv {
                flag: "--mcp".to_string(),
            },
            McpSlot::Workdir {
                path: PathBuf::from(".cursor/mcp.json"),
                format: McpFormat::CursorMcpJson,
            },
            McpSlot::Inline {
                flag: "-c".to_string(),
                key_prefix: "mcp_servers".to_string(),
            },
            McpSlot::Unsupported,
        ];

        // When
        let written: Vec<Result<Vec<PathBuf>, SlotError>> = slots
            .iter()
            .map(|slot| mcp_written_paths(slot, workdir, scratch, Some(home)))
            .collect();

        // Then
        assert_eq!(
            written,
            vec![
                Ok(vec![scratch.join("claude-mcp-config.json")]),
                Ok(vec![home.join(".cursor/mcp.json")]),
                Ok(vec![scratch.join(ARGV_MCP_FILE)]),
                Ok(vec![workdir.join(".cursor/mcp.json")]),
                Ok(Vec::new()),
                Ok(Vec::new()),
            ]
        );
    }
}
