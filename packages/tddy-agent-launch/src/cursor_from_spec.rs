//! What a `cursor-cli` launch resolves before it renders the `cursor` tool definition
//! (`tddy-coding-agent-specs/specs/cursor.yaml`) with `harness_from_spec::harness_template_params`
//! — on the host through [`crate::spec_launch::launch_host_pty`], in a jail through
//! [`crate::launch_ports::LaunchSessions::launch_jailed`]: the bundled runtime behind Cursor's
//! launcher script.
//!
//! PRD: `docs/ft/daemon/cursor-cli-session.md` (`#agent-tools 7/12`).
//!
//! The chat id is the one the session owns for its whole lifetime (`agent create-chat`, persisted
//! as `cursor_chat_id`), so a start and every resume render the same template with the same id.

use std::path::{Path, PathBuf};

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
    node_launcher_of_on(&tddy_fs::DiskFs, resolved_binary)
}

/// [`node_launcher_of`] through the filesystem port.
pub fn node_launcher_of_on(fs: &dyn tddy_fs::Fs, resolved_binary: &Path) -> Option<NodeLauncher> {
    let binary = fs.canonicalize(resolved_binary).ok()?;
    let install_dir = binary.parent()?;
    let node = install_dir.join("node");
    let entry = install_dir.join("index.js");
    // `metadata_follow`, as before: `is_file` follows a link.
    let is_entry_file =
        matches!(fs.metadata_follow(&entry), Ok(m) if m.kind == tddy_fs::FsKind::File);
    (is_executable_file(fs, &node) && is_entry_file).then_some(NodeLauncher { node, entry })
}

/// Whether `path` is a regular file with an execute bit set.
fn is_executable_file(fs: &dyn tddy_fs::Fs, path: &Path) -> bool {
    // `metadata_follow`, as before: the question stays about the file itself.
    fs.metadata_follow(path)
        .is_ok_and(|m| m.kind == tddy_fs::FsKind::File && m.mode.unwrap_or(0) & 0o111 != 0)
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
