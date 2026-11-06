//! The session pointer (`#agent-tools 15/19`): how a `tddy-tools` a headless agent runs finds its
//! session without any `TDDY_*` variable in the agent's environment.
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.
//!
//! Before each headless turn the backend writes [`SESSION_POINTER_FILE`] in the agent's working
//! directory — the session's toolcall socket, its repo and its session directory, and the remote
//! tool environment when the session has one — git-excludes it through `info/exclude`, and removes
//! it when the turn ends ([`crate::turn_files::TurnFiles`]). A `tddy-tools` run from the agent's
//! shell, or as its MCP server (which runs in the agent's cwd), walks up from its cwd to the file
//! ([`find_session_pointer`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where the pointer is written, relative to the agent's working directory.
pub const SESSION_POINTER_FILE: &str = ".tddy/session.json";

/// What a `tddy-tools` run by a headless agent, or from the shell of a daemon-launched CLI agent,
/// needs to reach its session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPointer {
    /// The session's toolcall socket (what `TDDY_SOCKET` named); `None` for a session with no
    /// workflow listener whose pointer only carries [`Self::notify`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket: Option<PathBuf>,
    /// The repo the agent works in (what `TDDY_REPO_DIR` named).
    pub repo_dir: PathBuf,
    /// The session's directory (what `TDDY_SESSION_DIR` named); `None` for a session with none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_dir: Option<PathBuf>,
    /// The remote tool environment (`RemoteToolEnv::env_pairs`), by variable name; empty for a
    /// session with no remote.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub remote: BTreeMap<String, String>,
    /// What `tddy-tools notify` reads its session from (`session_activity::notify_env`: the session
    /// id, OS user, hook token and daemon address), by variable name; empty for an agent that does
    /// not notify its operator. The agent process is never given these as variables
    /// (`#agent-tools 17/19`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub notify: BTreeMap<String, String>,
    /// The process that owns the pointer's session, and the host it runs on (`#agent-tools 21/28`):
    /// a `tddy-tools` on that host refuses the pointer once the process is gone, and a backend does
    /// not overwrite a pointer a live owner holds for another session. `None` for a pointer written
    /// before owners were recorded, which is read as it always was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<SessionPointerOwner>,
}

/// The process a pointer's session is served by: the daemon for a pointer on its own host, the
/// agent on an ssh target for a pointer there (`#agent-tools 21/28`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPointerOwner {
    /// Its pid, on [`Self::host`].
    pub pid: u32,
    /// The host the pid lives on, as that host names itself ([`this_host`]).
    pub host: String,
}

impl SessionPointerOwner {
    /// This process, on this host.
    pub fn this_process() -> Self {
        Self {
            pid: std::process::id(),
            host: this_host(),
        }
    }

    /// Whether the owner's pid lives on the host this process runs on — only then can its liveness,
    /// or the pointer's socket path, be checked here.
    pub fn is_on_this_host(&self) -> bool {
        self.host == this_host()
    }

    /// Whether the owner's process is still running. Only meaningful when
    /// [`Self::is_on_this_host`]: a pid says nothing on another host.
    pub fn is_running(&self) -> bool {
        let Ok(pid) = libc::pid_t::try_from(self.pid) else {
            return false;
        };
        // SAFETY: signal 0 only checks that the process exists and may be signalled.
        let status = unsafe { libc::kill(pid, 0) };
        // `EPERM`: it exists, and belongs to another user.
        status == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

/// The name this host gives itself (`gethostname`, as `hostname` prints it).
pub fn this_host() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is valid for `buf.len()` bytes; `gethostname` NUL-terminates within it or fails.
    let status = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if status != 0 {
        return String::new();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

/// A pointer for another session is in the way: its owner is alive, so it is not overwritten
/// (`#agent-tools 21/28`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPointerHeld {
    /// The pointer file.
    pub path: PathBuf,
    /// The socket the held pointer names.
    pub socket: Option<PathBuf>,
    /// Its live owner.
    pub owner: SessionPointerOwner,
}

impl std::fmt::Display for SessionPointerHeld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let socket = self
            .socket
            .as_deref()
            .map_or_else(|| "no socket".to_string(), |s| s.display().to_string());
        write!(
            f,
            "the session pointer {} is held by another live session (pid {} on {}, {socket}), and is not overwritten",
            self.path.display(),
            self.owner.pid,
            self.owner.host
        )
    }
}

impl std::error::Error for SessionPointerHeld {}

/// Why a pointer file that exists could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPointerError {
    /// The file.
    pub path: PathBuf,
    /// What was wrong with it.
    pub reason: String,
}

impl std::fmt::Display for SessionPointerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the session pointer {} could not be read: {}",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for SessionPointerError {}

/// The nearest [`SESSION_POINTER_FILE`] at or above `start`, with its path: `None` when no
/// directory up to the filesystem root holds one. A file that exists but does not parse is an
/// error, never skipped for one further up.
pub fn find_session_pointer(
    start: &Path,
) -> Result<Option<(PathBuf, SessionPointer)>, SessionPointerError> {
    for dir in start.ancestors() {
        let path = dir.join(SESSION_POINTER_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => continue,
            Err(e) => {
                return Err(SessionPointerError {
                    path,
                    reason: e.to_string(),
                })
            }
        };
        return match serde_json::from_str::<SessionPointer>(&text) {
            Ok(pointer) => Ok(Some((path, pointer))),
            Err(e) => Err(SessionPointerError {
                path,
                reason: e.to_string(),
            }),
        };
    }
    Ok(None)
}
