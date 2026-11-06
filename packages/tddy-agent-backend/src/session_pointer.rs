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

/// What a `tddy-tools` run by a headless agent needs to reach its session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPointer {
    /// The session's toolcall socket (what `TDDY_SOCKET` named).
    pub socket: PathBuf,
    /// The repo the agent works in (what `TDDY_REPO_DIR` named).
    pub repo_dir: PathBuf,
    /// The session's directory (what `TDDY_SESSION_DIR` named); `None` for a session with none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_dir: Option<PathBuf>,
    /// The remote tool environment (`RemoteToolEnv::env_pairs`), by variable name; empty for a
    /// session with no remote.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub remote: BTreeMap<String, String>,
}

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
