//! Cross-platform sandbox abstraction for running confined agent processes.
//!
//! Platform-specific spawn is provided by `tddy-sandbox-darwin` on macOS.
//! On other platforms, [`spawn`] returns [`SandboxError::Unsupported`].

mod context_dir;
mod error;
mod log;
mod spec;

pub use context_dir::{SandboxContextDir, SANDBOX_REMOTE_APPENDIX};
pub use error::SandboxError;
pub use log::{
    append_line, egress_log_path, format_egress_logs, format_sandbox_diagnostics,
    SANDBOX_EXEC_STDERR_LOG, SANDBOX_EXEC_STDOUT_LOG, SANDBOX_RUNNER_FAILURE, SANDBOX_RUNNER_LOG,
    SANDBOX_SPAWN_MANIFEST,
};
pub use spec::{SandboxHandle, SandboxSpec};

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
