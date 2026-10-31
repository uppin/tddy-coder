//! What a `cursor-cli` launch binds when it renders the `cursor` tool definition
//! (`tddy-coding-agent-specs/specs/cursor.yaml`) — on the host through
//! [`crate::spec_launch::launch_host_pty`], in a jail through
//! [`crate::launch_ports::LaunchSessions::launch_jailed`].
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-cursor-cli-from-spec.md` (`#agent-tools 7/12`).
//!
//! Nothing here builds an argv: these are the template parameters `cursor.yaml` names. The chat id
//! is the one the session owns for its whole lifetime (`agent create-chat`, persisted as
//! `cursor_chat_id`), so a start and every resume render the same template with the same id.

use std::path::{Path, PathBuf};

use tddy_actions::tool::TemplateParams;

/// The `cursor` tool's id in the agent registry.
pub const CURSOR_AGENT_ID: &str = "cursor";

/// The bundled runtime a Cursor launcher script execs: `<install>/node` against
/// `<install>/index.js`. Run directly in a jail, where the script cannot resolve its own install
/// directory (`realpath "$0"` of a basename argv0 is the jail's cwd).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLauncher {
    pub node: PathBuf,
    pub entry: PathBuf,
}

/// The [`NodeLauncher`] behind `resolved_binary` when it is Cursor's launcher script — its
/// canonical directory holds an executable `node` and an `index.js` — and `None` for any other
/// binary, which then runs as itself.
pub fn node_launcher_of(resolved_binary: &Path) -> Option<NodeLauncher> {
    // TODO(cursor-cli): implement — the layout `build_cursor_sandbox_argv` probes today.
    let _ = resolved_binary;
    todo!("cursor-cli: node_launcher_of")
}

/// The parameters `cursor.yaml`'s `run.argv` is rendered with: `chat_id`, `model` and `prompt`
/// each bound when non-empty (trimmed), and `node_launcher` / `node_entry` when `launcher` is set.
pub fn cursor_template_params(
    chat_id: &str,
    model: &str,
    prompt: Option<&str>,
    launcher: Option<&NodeLauncher>,
) -> TemplateParams {
    // TODO(cursor-cli): implement.
    let _ = (chat_id, model, prompt, launcher);
    todo!("cursor-cli: cursor_template_params")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn a_start_binds_the_chat_model_and_prompt_it_was_given() {
        // When
        let params = cursor_template_params("chat-1", "composer-2.5", Some("  fix it  "), None);

        // Then
        assert_eq!(
            params,
            TemplateParams::new()
                .with("chat_id", "chat-1")
                .with("model", "composer-2.5")
                .with("prompt", "fix it")
        );
    }

    #[test]
    fn an_empty_model_and_prompt_are_left_unbound() {
        // When
        let params = cursor_template_params("chat-1", "", Some("   "), None);

        // Then
        assert_eq!(params, TemplateParams::new().with("chat_id", "chat-1"));
    }

    #[test]
    fn a_launcher_binds_its_node_and_entry() {
        // Given
        let launcher = NodeLauncher {
            node: PathBuf::from("/opt/cursor-agent/versions/2026.10.1/node"),
            entry: PathBuf::from("/opt/cursor-agent/versions/2026.10.1/index.js"),
        };

        // When
        let params = cursor_template_params("chat-1", "", None, Some(&launcher));

        // Then
        assert_eq!(
            params,
            TemplateParams::new()
                .with("chat_id", "chat-1")
                .with("node_launcher", "/opt/cursor-agent/versions/2026.10.1/node")
                .with(
                    "node_entry",
                    "/opt/cursor-agent/versions/2026.10.1/index.js"
                )
        );
    }

    #[test]
    fn a_launcher_script_beside_a_bundled_node_and_index_js_is_unwrapped() {
        // Given — Cursor's install layout: `agent` beside `node` and `index.js`
        let install = tempfile::tempdir().unwrap();
        let install_dir = std::fs::canonicalize(install.path()).unwrap();
        for name in ["agent", "node"] {
            std::fs::write(install_dir.join(name), "#!/bin/sh\n").unwrap();
            make_executable(&install_dir.join(name));
        }
        std::fs::write(install_dir.join("index.js"), "").unwrap();

        // When
        let launcher = node_launcher_of(&install_dir.join("agent"));

        // Then
        assert_eq!(
            launcher,
            Some(NodeLauncher {
                node: install_dir.join("node"),
                entry: install_dir.join("index.js"),
            })
        );
    }

    #[test]
    fn a_binary_with_no_bundled_node_runs_as_itself() {
        // Given — the positive control's directory without `node`
        let install = tempfile::tempdir().unwrap();
        let install_dir = std::fs::canonicalize(install.path()).unwrap();
        std::fs::write(install_dir.join("agent"), "#!/bin/sh\n").unwrap();
        make_executable(&install_dir.join("agent"));
        std::fs::write(install_dir.join("index.js"), "").unwrap();

        // When
        let launcher = node_launcher_of(&install_dir.join("agent"));

        // Then
        assert_eq!(launcher, None);
    }
}
