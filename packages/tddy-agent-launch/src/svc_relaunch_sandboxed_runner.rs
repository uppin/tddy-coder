use std::path::PathBuf;

/// A relaunched managed session's controller, its orchestration prompt file and its host-side env.
pub type RelaunchManagedEnv = (
    Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

pub use crate::{relaunch_jail_dirs, relaunch_jail_steps};
