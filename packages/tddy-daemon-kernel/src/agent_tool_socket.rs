//! Where an embedded daemon's agent-facing tool socket lives.
//!
//! The path is needed on both sides and neither can ask the other: `tddy-daemon` binds it at
//! startup, and `tddy-session-lifecycle` tells a spawning agent about it. It lives here because
//! that is the crate they share — `tddy-daemon` depends on `tddy-session-lifecycle`, so the
//! dependency cannot run the other way.

use std::path::{Path, PathBuf};

/// Where an embedded daemon puts its agent-facing socket.
///
/// Under `$TMPDIR` rather than beside the session data, because an `AF_UNIX` path is limited to
/// about 104 bytes on macOS and a checkout path alone can spend most of that — the same constraint
/// `tddy-index-daemon` works around by keying its socket on a digest instead of nesting it under
/// the worktree. `key` (the daemon's data dir) is digested so two daemons on one machine cannot
/// collide, and so the path stays the same length whatever the data dir is.
pub fn agent_tool_socket_path(key: &Path) -> PathBuf {
    let digest = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        h.finish()
    };
    let tmp = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    tmp.join(format!("tddy-agent-tools-{digest:016x}.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_path_is_short_enough_for_af_unix_whatever_the_data_dir_is() {
        // Given — a data dir as long as a real worktree checkout's, which is what overflowed the
        // limit when the socket was nested under it
        let long = Path::new(
            "/Users/someone/.superset/worktrees/tddy-coder/a-branch-name-that-is-quite-long/tmp/.tddy",
        );

        let path = agent_tool_socket_path(long);

        assert!(
            path.as_os_str().len() < 104,
            "AF_UNIX paths are capped near 104 bytes; got {} ({})",
            path.as_os_str().len(),
            path.display()
        );
    }

    #[test]
    fn two_daemons_with_different_data_dirs_do_not_share_a_socket() {
        let a = agent_tool_socket_path(Path::new("/one/.tddy"));
        let b = agent_tool_socket_path(Path::new("/two/.tddy"));

        assert_ne!(a, b, "a shared socket would cross two daemons' sessions");
    }

    #[test]
    fn the_same_daemon_resolves_the_same_socket_every_time() {
        // The agent is told this path at spawn and the daemon binds it at startup; they must agree
        // without either passing it to the other.
        let key = Path::new("/some/data/dir/.tddy");

        assert_eq!(agent_tool_socket_path(key), agent_tool_socket_path(key));
    }
}
