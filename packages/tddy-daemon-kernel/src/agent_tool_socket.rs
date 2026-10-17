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

/// The agent-facing socket of the daemon keeping its state in `data_dir`, when that daemon is
/// embedded in an application and therefore serves no HTTP listener for a co-located process to
/// POST to.
///
/// Decided by whether something is *listening* on the socket rather than by a host-kind flag
/// threaded through the session layer: an embedded daemon binds it at startup
/// (`tddy_daemon::runtime`), a binary one never does, and the path is derived from the same data
/// dir on both sides — so the question "does this host serve that socket" is answered by the host
/// itself, and a stale flag cannot disagree with reality. `data_dir` must be the one the daemon
/// *resolved* (`TDDY_DATA_DIR` included), not a second reading of the configuration: the path is a
/// digest of it.
///
/// A connect probe, not `exists()`: the embedded daemon does not unlink its socket on exit, so
/// once Tddy Desktop has run, the file outlives it. A binary daemon later started on the same data
/// dir would otherwise find that file and point every hook at a socket nobody serves — and the
/// hooks' fail-quiet contract would swallow every report. The probe connection is closed at once;
/// the daemon's endpoint sees EOF before any frame and ends it without a word.
pub fn embedded_host_agent_tool_socket(data_dir: &Path) -> Option<String> {
    let path = agent_tool_socket_path(data_dir);
    is_listening(&path).then(|| path.to_string_lossy().into_owned())
}

/// Whether a process is accepting connections on the Unix socket at `path`.
fn is_listening(path: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(path).is_ok()
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

    /// A daemon's data dir, unique per test so the `$TMPDIR` socket paths digested from it never
    /// collide. Whatever socket file a test leaves at that path is removed on drop.
    struct ADataDir {
        dir: tempfile::TempDir,
    }

    impl ADataDir {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().expect("a temporary data dir"),
            }
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }

        fn socket_path(&self) -> PathBuf {
            agent_tool_socket_path(self.path())
        }

        /// Bind its agent-tool socket, as an embedded daemon does at startup.
        fn with_its_socket_listening(&self) -> std::os::unix::net::UnixListener {
            std::os::unix::net::UnixListener::bind(self.socket_path())
                .expect("the agent-tool socket binds")
        }

        /// Leave its socket file behind with nobody listening — what an embedded daemon that
        /// exited leaves: binding creates the file, and dropping the listener does not unlink it.
        fn with_a_stale_socket_file(&self) {
            drop(self.with_its_socket_listening());
            assert!(
                self.socket_path().exists(),
                "the socket file outlives its listener"
            );
        }
    }

    impl Drop for ADataDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.socket_path());
        }
    }

    #[test]
    fn a_host_listening_on_its_agent_tool_socket_is_reached_over_it() {
        // Given an embedded daemon listening on its agent-tool socket
        let data_dir = ADataDir::new();
        let _listener = data_dir.with_its_socket_listening();

        // When a session asks where its hooks should report
        let socket = embedded_host_agent_tool_socket(data_dir.path());

        // Then they report over that socket
        assert_eq!(
            socket,
            Some(data_dir.socket_path().to_string_lossy().into_owned())
        );
    }

    #[test]
    fn a_host_without_an_agent_tool_socket_is_not_reached_over_one() {
        // Given a binary daemon, which never binds the socket
        let data_dir = ADataDir::new();

        // When a session asks where its hooks should report
        let socket = embedded_host_agent_tool_socket(data_dir.path());

        // Then there is no socket to report over
        assert_eq!(socket, None);
    }

    #[test]
    fn a_socket_file_nobody_listens_on_is_not_reached_over() {
        // Given the socket file an exited embedded daemon left behind for the same data dir
        let data_dir = ADataDir::new();
        data_dir.with_a_stale_socket_file();

        // When a session asks where its hooks should report
        let socket = embedded_host_agent_tool_socket(data_dir.path());

        // Then the dead socket is not chosen: every report sent to it would be lost
        assert_eq!(socket, None);
    }
}
