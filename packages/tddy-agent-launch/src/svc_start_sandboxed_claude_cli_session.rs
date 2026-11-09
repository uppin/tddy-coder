use std::path::{Path, PathBuf};

/// The session a sandboxed start is building a jail for: who it is and where it lives on the host.
pub struct JailSession<'a> {
    pub session_id: &'a str,
    pub project_id: &'a str,
    /// The caller's session token — what the session's tools' account lookups read the vault with.
    pub session_token: &'a str,
    pub session_dir: &'a Path,
    pub worktree_path: &'a Path,
}

/// The per-session jail directories, created and resolved to their canonical paths.
pub struct JailDirs {
    pub sandbox_root: PathBuf,
    pub egress_dir: PathBuf,
    pub scratch_dir: PathBuf,
    pub scratch_tmp: PathBuf,
    pub context_dir: PathBuf,
}

/// A managed-workflow session's controller, its orchestration prompt file and its host-side env.
pub type ManagedJailEnv = (
    Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

pub use crate::{jail_env_builders, jail_launch_steps, jail_session_files};
