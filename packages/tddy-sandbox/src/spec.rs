use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Child;

/// Specification for spawning a confined process.
#[derive(Debug, Clone)]
pub struct SandboxSpec {
    /// Root of the sandbox project tree (writable).
    pub project_root: PathBuf,
    /// Scratch directory inside the project (HOME/TMPDIR redirected here).
    pub scratch_dir: PathBuf,
    /// Egress/output directory (writable).
    pub egress_dir: PathBuf,
    /// Additional paths allowed for read-only access (toolchains).
    pub allow_read_paths: Vec<PathBuf>,
    /// Command and arguments to run inside the sandbox.
    pub command: Vec<String>,
    /// Environment variables for the inner process (clean env recommended).
    pub env: BTreeMap<String, String>,
    /// Path where the rendered SBPL profile is written (darwin only).
    pub profile_path: PathBuf,
    /// Loopback TCP ports the confined process may bind and connect to (not external egress).
    pub loopback_allow_ports: Vec<u16>,
    /// Optional short, canonical path for the in-jail tool-IPC AF_UNIX socket, granted an
    /// explicit read+write allow. Kept separate from the (deep) session tree because macOS
    /// caps `sockaddr_un.sun_path` at `SUN_LEN` (104 bytes); a socket under the canonical
    /// session dir overflows it. `None` falls back to the project tree's allows.
    pub ipc_socket: Option<PathBuf>,
    /// Working directory for the confined process. Defaults to [`project_root`](Self::project_root).
    pub cwd: Option<PathBuf>,
}

impl SandboxSpec {
    /// Pick a short, canonical AF_UNIX socket path that fits within `SUN_LEN` (104 bytes on
    /// macOS), regardless of how deep the session directory is. Lives under the real per-user
    /// temp (`std::env::temp_dir()`, canonicalized) with a short session-derived name.
    pub fn short_ipc_socket_path(session_id: &str) -> PathBuf {
        let tmp =
            std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        // FNV-1a 64 of the whole id: the first characters of a UUIDv7 are its millisecond
        // timestamp, which two sessions can share, and no hashing crate is needed for this.
        let hash = session_id
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        tmp.join(format!("tddy-{hash:016x}-{}.sock", std::process::id()))
    }
}

impl SandboxSpec {
    pub fn validate(&self) -> Result<(), crate::SandboxError> {
        if self.command.is_empty() {
            return Err(crate::SandboxError::InvalidSpec(
                "command must not be empty".to_string(),
            ));
        }
        if !self.project_root.is_absolute() {
            return Err(crate::SandboxError::InvalidSpec(
                "project_root must be absolute".to_string(),
            ));
        }
        if let Some(ref cwd) = self.cwd {
            if !cwd.is_absolute() {
                return Err(crate::SandboxError::InvalidSpec(
                    "cwd must be absolute".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Handle to a running sandboxed child process.
pub struct SandboxHandle {
    child: Child,
    pub profile_path: PathBuf,
    pub grpc_socket_path: PathBuf,
    pub ready_marker_path: PathBuf,
}

impl SandboxHandle {
    pub fn new(
        child: Child,
        profile_path: PathBuf,
        grpc_socket_path: PathBuf,
        ready_marker_path: PathBuf,
    ) -> Self {
        Self {
            child,
            profile_path,
            grpc_socket_path,
            ready_marker_path,
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub fn into_child(self) -> Child {
        self.child
    }

    /// Take ownership of the child's piped stdin/stdout. Only `Some` when the spawning platform
    /// crate piped them (i.e. the command included `--stdio`) instead of routing stdout to an
    /// egress log — see `tddy-sandbox-darwin::spawn_plan`.
    pub fn take_stdio(&mut self) -> Option<(std::process::ChildStdin, std::process::ChildStdout)> {
        match (self.child.stdin.take(), self.child.stdout.take()) {
            (Some(stdin), Some(stdout)) => Some((stdin, stdout)),
            _ => None,
        }
    }

    /// If the sandboxed child has already exited, return a human-readable reason.
    ///
    /// Returns `None` while the child is still running. This is the key signal that was
    /// missing during the seatbelt SIGABRT investigation: a child that dies in `dyld`
    /// before `main()` never writes a ready marker, so a marker-only wait blocks for the
    /// full timeout instead of surfacing the abort. Decodes termination-by-signal and
    /// adds a hint for `SIGABRT` (abort trap 6 / exit 134), the classic signature of a
    /// dyld shared-cache read being denied by the SBPL profile.
    pub fn try_exit_diagnostic(&mut self) -> Option<String> {
        use std::os::unix::process::ExitStatusExt;
        let status = self.child.try_wait().ok().flatten()?;
        if let Some(signal) = status.signal() {
            let mut msg = format!(
                "sandboxed child (pid {}) terminated by signal {signal}",
                self.child.id()
            );
            if signal == 6 {
                msg.push_str(
                    " (SIGABRT / abort trap 6). This is typically dyld aborting in \
                     CacheFinder before main() because the SBPL profile does not permit \
                     reading the dyld shared cache — ensure the file-read allow-list \
                     includes (literal \"/\").",
                );
            }
            Some(msg)
        } else {
            Some(format!(
                "sandboxed child (pid {}) exited with code {:?}",
                self.child.id(),
                status.code()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SandboxSpec;

    /// Two UUIDv7 session ids minted in the same millisecond: their first ten characters — the
    /// timestamp — are the same, and only the random tail tells them apart.
    const FIRST_SESSION: &str = "01a1b24a-f0b0-7501-8699-f9dc1f9f0acc";
    const SECOND_SESSION: &str = "01a1b24a-f0b0-7a3c-b2d1-0c4e8f7a9b12";

    /// The file name of `session_id`'s tool-IPC socket.
    fn socket_name_of(session_id: &str) -> String {
        SandboxSpec::short_ipc_socket_path(session_id)
            .file_name()
            .expect("a socket path names a file")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn two_sessions_minted_in_the_same_millisecond_get_different_tool_ipc_sockets() {
        // Given two jails of this daemon, for sessions minted in the same millisecond

        // When
        let first = SandboxSpec::short_ipc_socket_path(FIRST_SESSION);
        let second = SandboxSpec::short_ipc_socket_path(SECOND_SESSION);

        // Then
        assert_ne!(first, second);
    }

    #[test]
    fn the_tool_ipc_socket_is_named_by_an_fnv64_of_the_whole_session_id_and_the_daemons_pid() {
        // Given
        // FNV-1a 64 of FIRST_SESSION, computed independently of the code under test.
        let fnv64_of_the_first_session = "69d37be2277167c1";

        // When
        let name = socket_name_of(FIRST_SESSION);

        // Then
        assert_eq!(
            name,
            format!(
                "tddy-{fnv64_of_the_first_session}-{}.sock",
                std::process::id()
            )
        );
    }

    #[test]
    fn the_tool_ipc_socket_of_a_long_session_id_still_fits_sun_len() {
        // Given — an id far longer than a UUID
        let long_session_id = "s".repeat(400);

        // When
        let socket = SandboxSpec::short_ipc_socket_path(&long_session_id);

        // Then — macOS caps `sun_path` at 104 bytes, its terminating NUL included. A bound, not an
        // exact length: the temp dir the path lives under differs per host.
        assert!(
            socket.as_os_str().len() < 104,
            "{} is {} bytes",
            socket.display(),
            socket.as_os_str().len()
        );
    }
}
