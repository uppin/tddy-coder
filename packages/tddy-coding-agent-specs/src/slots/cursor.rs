//! Cursor's slots (`#agent-tools 7/12`): the `cursor-mcp-json` registration placed in the agent's
//! home, the managed-workflow rules file, and the `cursor-hooks-json` hooks adapter.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-cursor-cli-from-spec.md`.
//!
//! The hooks adapter reports over the daemon's agent-tool socket and nothing else: on the host it
//! is the socket the daemon serves co-located agents on, and in a jail the in-jail
//! `tddy-tools session-hook` reaches the daemon over the tool-IPC bridge instead. Params without a
//! socket are refused ([`SlotError::NoDaemonSocket`]) — the silent `daemon_url` fallback is what
//! made every desktop Cursor hook a swallowed 404
//! (`docs/dev/todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md`).

use std::path::{Path, PathBuf};

use tddy_agent_backend::claude_hooks::HookCommandParams;

use super::hooks::HooksAdapter;
use super::{McpServerCommand, SlotError};

/// [`CursorHooksJson`]'s name.
pub const CURSOR_HOOKS_JSON: &str = "cursor-hooks-json";

/// Where Cursor reads its lifecycle hooks, relative to the agent's working directory.
pub const CURSOR_HOOKS_FILE: &str = ".cursor/hooks.json";

/// Cursor Agent's `<workdir>/.cursor/hooks.json`, each event wired to
/// `tddy-tools session-hook --daemon-socket <socket>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorHooksJson;

impl HooksAdapter for CursorHooksJson {
    fn name(&self) -> &'static str {
        CURSOR_HOOKS_JSON
    }

    /// Reached only through [`HooksAdapter::try_render`]: params without a daemon socket have no
    /// rendering, and a direct call with them panics naming the refusal.
    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>) {
        // TODO(cursor-cli): implement — `self.try_render(..)`, panicking with its refusal.
        let _ = (workdir, params);
        todo!("cursor-cli: CursorHooksJson::render")
    }

    fn try_render(
        &self,
        workdir: &Path,
        params: &HookCommandParams<'_>,
    ) -> Result<(PathBuf, Vec<u8>), SlotError> {
        // TODO(cursor-cli): implement — `render_cursor_hooks_json`.
        let _ = (workdir, params);
        todo!("cursor-cli: CursorHooksJson::try_render")
    }
}

/// `<workdir>/`[`CURSOR_HOOKS_FILE`] for `params`, pretty-printed: the five Cursor events
/// (`sessionStart`, `beforeSubmitPrompt`, `postToolUse`, `stop`, `sessionEnd`), each one command
/// that reports over `params.daemon_socket`. `params` without a socket is
/// [`SlotError::NoDaemonSocket`].
pub fn render_cursor_hooks_json(
    workdir: &Path,
    params: &HookCommandParams<'_>,
) -> Result<(PathBuf, Vec<u8>), SlotError> {
    // TODO(cursor-cli): implement — `tddy_agent_backend::build_cursor_hooks_settings`, which then
    // takes the socket it bakes in as a required argument.
    let _ = (workdir, params);
    todo!("cursor-cli: render_cursor_hooks_json")
}

/// The `cursor-mcp-json` registration of `server` at `path`: `{"mcpServers": {name: {command,
/// args, env?}}}`, compact, `env` omitted when empty — the bytes the runner's
/// `write_cursor_mcp_config` writes today.
pub fn render_cursor_mcp_json(server: &McpServerCommand, path: &Path) -> (PathBuf, Vec<u8>) {
    // TODO(cursor-cli): implement.
    let _ = (server, path);
    todo!("cursor-cli: render_cursor_mcp_json")
}

/// A placed registration `path` with `{home}` replaced by `home`; a path naming `{home}` with no
/// home bound is [`SlotError::UnboundHome`].
pub fn placed_path(path: &str, home: Option<&Path>) -> Result<PathBuf, SlotError> {
    // TODO(cursor-cli): implement.
    let _ = (path, home);
    todo!("cursor-cli: placed_path")
}

/// `<workdir>/<rules_file>` holding `preamble` and a final newline — written whole on every launch,
/// so a re-run replaces it rather than growing it.
pub fn render_rules_file(workdir: &Path, rules_file: &Path, preamble: &str) -> (PathBuf, Vec<u8>) {
    // TODO(cursor-cli): implement.
    let _ = (workdir, rules_file, preamble);
    todo!("cursor-cli: render_rules_file")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;

    use super::*;

    const SOCKET: &str = "/var/folders/xy/T/tddy-agent-tools-1a2b.sock";

    fn hook_params(daemon_socket: Option<&'static str>) -> HookCommandParams<'static> {
        HookCommandParams {
            tddy_tools_path: "/opt/tddy/bin/tddy-tools",
            daemon_url: "http://127.0.0.1:8899",
            session_id: "0190-c",
            os_user: "dev",
            hook_token: "token-c",
            daemon_socket,
            answer_wait_secs: 120,
        }
    }

    /// The `session-hook` command every Cursor event runs, reporting over [`SOCKET`].
    fn the_socket_hook_command() -> String {
        format!(
            "'/opt/tddy/bin/tddy-tools' session-hook --session 0190-c --daemon http://127.0.0.1:8899 --daemon-socket '{SOCKET}' --os-user dev --hook-token token-c"
        )
    }

    #[test]
    fn the_cursor_mcp_json_registers_the_server_command_at_the_placed_path() {
        // Given
        let server = McpServerCommand {
            name: "tddy-tools".to_string(),
            command: PathBuf::from("/opt/tddy/bin/tddy-tools"),
            args: vec!["--mcp".to_string()],
            env: BTreeMap::from([("TDDY_SESSION_ID".to_string(), "0190-c".to_string())]),
        };
        let path = PathBuf::from("/homes/cursor/.cursor/mcp.json");

        // When
        let rendered = render_cursor_mcp_json(&server, &path);

        // Then
        let expected = serde_json::json!({
            "mcpServers": {
                "tddy-tools": {
                    "command": "/opt/tddy/bin/tddy-tools",
                    "args": ["--mcp"],
                    "env": { "TDDY_SESSION_ID": "0190-c" },
                }
            }
        });
        assert_eq!(rendered, (path, expected.to_string().into_bytes()));
    }

    #[test]
    fn a_server_without_env_is_registered_without_an_env_block() {
        // Given
        let server = McpServerCommand {
            name: "tddy-tools".to_string(),
            command: PathBuf::from("/opt/tddy/bin/tddy-tools"),
            args: vec!["--mcp".to_string()],
            env: BTreeMap::new(),
        };
        let path = PathBuf::from("/homes/cursor/.cursor/mcp.json");

        // When
        let (_, bytes) = render_cursor_mcp_json(&server, &path);

        // Then
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            r#"{"mcpServers":{"tddy-tools":{"args":["--mcp"],"command":"/opt/tddy/bin/tddy-tools"}}}"#
        );
    }

    #[test]
    fn a_placed_path_resolves_home_to_the_launchs_home() {
        // When
        let resolved = placed_path(
            "{home}/.cursor/mcp.json",
            Some(Path::new("/tddy/homes/cursor")),
        );

        // Then
        assert_eq!(
            resolved,
            Ok(PathBuf::from("/tddy/homes/cursor/.cursor/mcp.json"))
        );
    }

    #[test]
    fn a_placed_path_naming_home_is_refused_when_no_home_is_bound() {
        // When
        let refused = placed_path("{home}/.cursor/mcp.json", None);

        // Then
        assert_eq!(
            refused,
            Err(SlotError::UnboundHome {
                path: "{home}/.cursor/mcp.json".to_string(),
            })
        );
    }

    #[test]
    fn the_rules_file_holds_the_preamble_at_its_declared_path() {
        // Given
        let workdir = Path::new("/worktrees/0190-c");

        // When
        let rendered = render_rules_file(
            workdir,
            Path::new(".cursor/rules/tddy-managed-workflow.mdc"),
            "Managed workflow recipe: tdd",
        );

        // Then
        assert_eq!(
            rendered,
            (
                PathBuf::from("/worktrees/0190-c/.cursor/rules/tddy-managed-workflow.mdc"),
                b"Managed workflow recipe: tdd\n".to_vec(),
            )
        );
    }

    #[test]
    fn the_hooks_json_wires_every_cursor_event_to_a_session_hook_over_the_given_socket() {
        // Given
        let workdir = Path::new("/worktrees/0190-c");

        // When
        let rendered = render_cursor_hooks_json(workdir, &hook_params(Some(SOCKET)));

        // Then
        let command = serde_json::json!([{ "command": the_socket_hook_command() }]);
        let expected = serde_json::json!({
            "version": 1,
            "hooks": {
                "sessionStart": command,
                "beforeSubmitPrompt": command,
                "postToolUse": command,
                "stop": command,
                "sessionEnd": command,
            }
        });
        assert_eq!(
            rendered,
            Ok((
                workdir.join(CURSOR_HOOKS_FILE),
                serde_json::to_string_pretty(&expected)
                    .unwrap()
                    .into_bytes(),
            ))
        );
    }

    #[test]
    fn the_hooks_json_is_refused_without_a_daemon_socket_rather_than_reporting_to_the_daemon_url() {
        // When
        let refused = render_cursor_hooks_json(Path::new("/worktrees/0190-c"), &hook_params(None));

        // Then
        assert_eq!(
            refused,
            Err(SlotError::NoDaemonSocket {
                adapter: CURSOR_HOOKS_JSON.to_string(),
            })
        );
    }

    #[test]
    fn the_cursor_adapter_renders_through_try_render_and_refuses_the_same_params_there() {
        // When
        let rendered = (
            CursorHooksJson.try_render(Path::new("/w"), &hook_params(Some(SOCKET))),
            CursorHooksJson.try_render(Path::new("/w"), &hook_params(None)),
        );

        // Then
        assert_eq!(
            rendered,
            (
                render_cursor_hooks_json(Path::new("/w"), &hook_params(Some(SOCKET))),
                Err(SlotError::NoDaemonSocket {
                    adapter: CURSOR_HOOKS_JSON.to_string(),
                }),
            )
        );
    }
}
