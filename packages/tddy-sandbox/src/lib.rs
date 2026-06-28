//! Cross-platform sandbox abstraction for running confined agent processes.
//!
//! Platform-specific spawn is provided by `tddy-sandbox-darwin` on macOS.
//! On other platforms, [`spawn`] returns [`SandboxError::Unsupported`].

pub mod builder;
pub mod claude_spawn;
mod context_dir;
mod error;
mod log;
pub mod materialize;
mod spec;
pub mod tool_ipc;

pub use builder::{
    CopySpec, EnvSpec, MachPolicy, MountSpec, NetworkSpec, PolicySpec, ReadKind, ReadReason,
    ReadSpec, ResourceLimits, SandboxBuilder, SandboxPlan, SecretSource, SecretSpec, SymlinkSpec,
};
pub use claude_spawn::{
    append_sandbox_claude_mcp_args, binary_exec_reads, build_sandbox_claude_allowlist,
    claude_policy, claude_required_copies, claude_required_reads, default_runner_env,
    detect_toolchain_reads, sandbox_claude_scratch_dir, system_baseline_reads,
    write_sandbox_mcp_config,
};
pub use context_dir::{
    copy_context_from_repo, copy_tree, copy_tree_within_root, SandboxContextDir,
    SANDBOX_REMOTE_APPENDIX,
};
pub use error::SandboxError;
pub use log::{
    append_line, egress_log_path, format_egress_logs, format_sandbox_diagnostics,
    SANDBOX_EXEC_STDERR_LOG, SANDBOX_EXEC_STDOUT_LOG, SANDBOX_RUNNER_FAILURE, SANDBOX_RUNNER_LOG,
    SANDBOX_SPAWN_MANIFEST,
};
pub use materialize::{materialize_copies, materialize_secrets, materialize_symlinks};
pub use spec::{SandboxHandle, SandboxSpec};
pub use tool_ipc::{session_id_from_env, ToolIpcRequest, ToolIpcResponse};

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
