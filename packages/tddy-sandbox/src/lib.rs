//! Cross-platform sandbox abstraction for running confined processes.
//!
//! Platform-specific spawn is provided by `tddy-sandbox-darwin` on macOS.
//! Product-specific read/copy/policy recipes live in `tddy-sandbox-recipes`.

pub mod builder;
mod context_dir;
mod error;
pub mod exec_reads;
mod log;
pub mod materialize;
pub mod runner_env;
mod spec;
pub mod tool_ipc;

pub use builder::{
    CopySpec, EnvSpec, MachPolicy, MountSpec, NetworkSpec, PolicySpec, ReadKind, ReadReason,
    ReadSpec, ResourceLimits, SandboxBuilder, SandboxPlan, SecretSource, SecretSpec, SymlinkSpec,
};
pub use context_dir::{
    copy_context_from_repo, copy_tree, copy_tree_within_root, SandboxContextDir,
    SubagentReplacement, SANDBOX_REMOTE_APPENDIX,
};
pub use error::SandboxError;
pub use exec_reads::{
    binary_exec_reads, detect_toolchain_reads, path_traversal_reads, process_exec_reads,
    system_baseline_reads,
};
pub use log::{
    append_line, egress_log_path, format_egress_logs, format_sandbox_diagnostics,
    SANDBOX_EXEC_STDERR_LOG, SANDBOX_EXEC_STDOUT_LOG, SANDBOX_RUNNER_FAILURE, SANDBOX_RUNNER_LOG,
    SANDBOX_SPAWN_MANIFEST,
};
pub use materialize::{materialize_copies, materialize_secrets, materialize_symlinks};
pub use runner_env::{process_jail_env, scratch_runner_env};
pub use spec::{SandboxHandle, SandboxSpec};
pub use tool_ipc::session_id_from_env;

/// Exec tool names served by the daemon `ExecuteTool` RPC for workspace/sandbox sessions.
///
/// Must stay in sync with `tddy_daemon::tool_catalog::tool_catalog`.
pub fn workspace_exec_tool_names() -> &'static [&'static str] {
    &[
        "Read",
        "Write",
        "StrReplace",
        "Delete",
        "Grep",
        "Glob",
        "Shell",
        "Await",
        "ReadLints",
        "SemanticSearch",
    ]
}

/// Spawn a process inside a platform sandbox.
///
/// On macOS, callers should use `tddy_sandbox_darwin::spawn` directly.
/// This facade returns [`SandboxError::Unsupported`] on all platforms.
pub fn spawn(spec: SandboxSpec) -> Result<SandboxHandle, SandboxError> {
    let _ = spec;
    Err(SandboxError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        message: "use tddy_sandbox_darwin::spawn on macOS".to_string(),
    })
}
