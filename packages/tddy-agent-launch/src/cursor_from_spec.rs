//! What a `cursor-cli` launch resolves before it renders the `cursor` tool definition
//! (`tddy-coding-agent-specs/specs/cursor.yaml`) with `harness_from_spec::harness_template_params`
//! — on the host through [`crate::spec_launch::launch_host_pty`], in a jail through
//! [`crate::launch_ports::LaunchSessions::launch_jailed`]: the configured binary and the bundled
//! runtime behind Cursor's launcher script.
//!
//! PRD: `docs/ft/daemon/cursor-cli-session.md` (`#agent-tools 7/12`).
//!
//! The chat id is the one the session owns for its whole lifetime (`agent create-chat`, persisted
//! as `cursor_chat_id`), so a start and every resume render the same template with the same id.

use std::path::{Path, PathBuf};

use tddy_daemon_kernel::config::{resolve_cursor_binary_path, DaemonConfig};

/// The `cursor` tool's id in the agent registry.
// TODO(cli-dispatch-collapse): deleted at green — § Removed of docs/dev/1-WIP/2026-11-04-agent-tools-cli-dispatch-collapse.md.
pub const CURSOR_AGENT_ID: &str = "cursor";

/// `cursor_cli.binary_path` (or `TDDY_CURSOR_AGENT`) when it names a path; a bare name leaves the
/// binary to the spec's own resolution.
pub(crate) fn configured_cursor_binary(config: &DaemonConfig) -> Option<PathBuf> {
    Some(resolve_cursor_binary_path(config))
        .filter(|path| path.contains('/'))
        .map(PathBuf::from)
}

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
    let binary = std::fs::canonicalize(resolved_binary).ok()?;
    let install_dir = binary.parent()?;
    let node = install_dir.join("node");
    let entry = install_dir.join("index.js");
    (is_executable_file(&node) && entry.is_file()).then_some(NodeLauncher { node, entry })
}

/// Whether `path` is a regular file with an execute bit set.
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
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
