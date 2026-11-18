//! A target's `~/.claude.json` made ready for a Claude start: the clone root trusted, and the
//! repository's project MCP servers recorded as rejected, so the session meets neither the
//! "trust this folder" nor the "use this MCP server" dialog on a terminal nobody is watching.
//!
//! Over the remote-shell port only: the `$HOME` of the target is probed, `~/.claude.json` is read
//! (an absent file is `{}`), `projects["<clone root>"]` gets `hasTrustDialogAccepted = true` and
//! every server of the repository's `.mcp.json` in `disabledMcpjsonServers` (with
//! `enabledMcpjsonServers` empty), every other key is kept, and the file is written back with
//! [`tddy_remote_shell::FileMode::OwnerOnly`] only when it changed.

use std::fmt;
use std::path::Path;

use tddy_remote_shell::{RemoteShell, RemoteShellError};

/// What [`pre_trust_clone_root`] did to the target's `~/.claude.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustOutcome {
    /// The file was written: it did not hold the trust and the MCP answer yet.
    Written,
    /// The file already held them: nothing was written.
    Unchanged,
}

/// Why the target's `~/.claude.json` could not be made ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetTrustError {
    /// The remote shell failed.
    Shell(RemoteShellError),
    /// The target's `~/.claude.json` or the repository's `.mcp.json` is not JSON of the shape
    /// Claude writes; `file` is the path on the target.
    Malformed { file: String, message: String },
}

impl fmt::Display for TargetTrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shell(error) => write!(f, "{error}"),
            Self::Malformed { file, message } => write!(f, "{file} is malformed: {message}"),
        }
    }
}

impl std::error::Error for TargetTrustError {}

/// Trust `clone_root` in the target's `~/.claude.json` and reject every server in the `.mcp.json`
/// of `worktree` (a worktree of that clone), keeping the file's other keys.
pub async fn pre_trust_clone_root(
    _shell: &dyn RemoteShell,
    _clone_root: &Path,
    _worktree: &Path,
) -> Result<TrustOutcome, TargetTrustError> {
    // TODO(target-agent-start): implement
    unimplemented!("TODO(target-agent-start): pre-trust the clone root on the target")
}
