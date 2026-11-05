use std::path::Path;

use std::path::PathBuf;

use tddy_core::provider_route::HarnessRoute;

/// What the relaunched runner's env is built from.
pub struct RelaunchJailEnv<'a> {
    pub session_id: &'a str,
    pub worktree_path: &'a Path,
    pub specialized_defs: Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
    pub egress_dir: &'a Path,
    pub scratch_tmp: PathBuf,
    pub scratch_home: &'a Path,
    pub tool_ipc_socket: &'a Path,
}

/// What the relaunched sandbox runner is spawned with.
pub struct RelaunchedRunnerSpawn<'a> {
    pub sandbox_root: PathBuf,
    pub egress_dir: &'a Path,
    pub scratch_dir: PathBuf,
    pub scratch_home: PathBuf,
    pub tool_ipc_socket: PathBuf,
    pub ready_marker: &'a Path,
    pub profile_path: PathBuf,
    pub loopback_allow_ports: Vec<u16>,
    pub runner_argv: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    /// The provider route the agent runs on, re-resolved for the resume; `None` for its vendor.
    pub route: Option<HarnessRoute>,
}

/// What bridging a relaunched jail into this daemon, and registering it, needs.
pub struct RelaunchedJailBridge<'a> {
    pub session_id: &'a str,
    pub session_dir: &'a Path,
    pub worktree_path: &'a Path,
    pub egress_dir: PathBuf,
    pub managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    pub session_env: Vec<(String, String)>,
    pub ready_marker: PathBuf,
    pub handle: tddy_sandbox::SandboxHandle,
}

/// A relaunched managed session's controller, its orchestration prompt file and its host-side env.
pub type RelaunchManagedEnv = (
    Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

pub use crate::{relaunch_jail_dirs, relaunch_jail_steps};
